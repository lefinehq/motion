// The lens blur (`lens.rs`): the running sums of every row of the layer
// (`RowSums`), then every pixel the mean of the layer over its iris, a
// stretch of each row it spans read from two running sums (`spread`).
//
// The CPU sums a row in 64-bit floats and keeps each running sum as the
// nearest 32-bit float. Here a row is summed as 64-bit fixed-point numbers
// of 2^-40, two 32-bit words a channel, in chunks at once (`row_chunks`,
// `row_scan`, `row_sums`), which integer arithmetic adds exactly in any
// order, and each running sum is rounded to the nearest float in integers
// too: the same floats the CPU keeps. A pixel's iris then reads only
// differences of them, as small floats.

struct Params {
    // The grid's width and height, its pixels, and how many invocations
    // one row of workgroups covers.
    width: u32,
    height: u32,
    count: u32,
    row: u32,
    // Pixels a chunk of a row, and chunks a row.
    chunk: u32,
    chunks: u32,
    // 1 when the edge pixels lie beyond the edges; when a map sets each
    // pixel's radius.
    repeat: u32,
    mapped: u32,
    // How many rows the iris spans at the one radius, without a map.
    rows: u32,
    // The map's level (luminance, red, green, blue, alpha), 1 when it is
    // taken from 1, and the map's size.
    level: u32,
    inverted: u32,
    map_width: u32,
    map_height: u32,
    // A map point's coefficients are whole numbers over 2^shift, and a rest.
    shift: u32,
    // Always 0, which the compiler cannot know: see `fenced`.
    zero: u32,
    pad1: u32,
    // The radius, the level in focus, the iris's weight at the one radius,
    // and the iris's top and bottom at a radius of 1.
    radius: f32,
    focal: f32,
    weight: f32,
    top: f32,
    bottom: f32,
    pad2: f32,
    pad3: f32,
    pad4: f32,
    // Where pixel (0, 0)'s centre reads the map, less half a px: its
    // whole px and part; and how far it moves for a px right and down
    // (x right, y right, x down, y down), as whole numbers over 2^shift and
    // the rest.
    map_whole: vec4<i32>,
    map_part: vec4<f32>,
    map_steps: vec4<i32>,
    map_rest: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> written: array<vec4<u32>>;
// The layer's pixels.
@group(0) @binding(2) var<storage, read> grid: array<vec4<f32>>;
// The chunks' exact sums, two vec4s each.
@group(0) @binding(3) var<storage, read> chunk_sums: array<vec4<u32>>;
// Every row's running sums, `width + 1` a row from 0.
@group(0) @binding(4) var<storage, read> sums: array<vec4<f32>>;
// The iris's rows at the one radius (two vec4s each), or its edges at
// `IRIS_ROWS` heights for a map (one each).
@group(0) @binding(5) var<storage, read> table: array<vec4<u32>>;
// The blur map's pixels.
@group(0) @binding(6) var<storage, read> map: array<vec4<f32>>;

const IRIS_ROWS: u32 = 1024u;

// ---- 64-bit fixed-point numbers, four channels at once ----

struct Wide {
    lo: vec4<u32>,
    hi: vec4<u32>,
}

fn wide_add(a: Wide, b: Wide) -> Wide {
    let lo = a.lo + b.lo;
    let carry = select(vec4(0u), vec4(1u), lo < a.lo);
    return Wide(lo, a.hi + b.hi + carry);
}

fn wide_sub(a: Wide, b: Wide) -> Wide {
    let lo = a.lo - b.lo;
    let borrow = select(vec4(0u), vec4(1u), a.lo < b.lo);
    return Wide(lo, a.hi - b.hi - borrow);
}

// `v` times 2^40, to the nearest whole number: its 24-bit significand
// shifted by its exponent. Below 2^-41 is nothing; at or past 128, just
// below it.
fn fixed(v: vec4<f32>) -> Wide {
    let bits = bitcast<vec4<u32>>(v);
    let e = (bits >> vec4(23u)) & vec4(255u);
    let normal = e >= vec4(86u);
    let big = e > vec4(133u);
    let m = select((bits & vec4(0x7fffffu)) | vec4(0x800000u), vec4(0xffffffu), big);
    let ee = select(e, vec4(133u), big);
    let left = ee >= vec4(110u);
    let ls = select(vec4(0u), ee - vec4(110u), left);
    let rs = select(vec4(110u) - ee, vec4(0u), left);
    let rounded = (m + select(vec4(0u), vec4(1u) << (rs - vec4(1u)), rs > vec4(0u))) >> rs;
    let lo = m << ls;
    let hi = select(vec4(0u), m >> ((vec4(32u) - ls) & vec4(31u)), ls > vec4(0u));
    var w = Wide(select(rounded, lo, left), select(vec4(0u), hi, left));
    w = Wide(select(vec4(0u), w.lo, normal), select(vec4(0u), w.hi, normal));
    let negative = (bits >> vec4(31u)) == vec4(1u);
    let flipped = wide_sub(Wide(vec4(0u), vec4(0u)), w);
    return Wide(select(w.lo, flipped.lo, negative), select(w.hi, flipped.hi, negative));
}

// One channel of a fixed-point number, rounded to the nearest float, ties
// to even, in integers.
fn nearest(lo_in: u32, hi_in: u32) -> f32 {
    let negative = (hi_in >> 31u) == 1u;
    var lo = lo_in;
    var hi = hi_in;
    if negative {
        lo = ~lo_in + 1u;
        hi = ~hi_in + select(0u, 1u, lo == 0u);
    }
    if hi == 0u && lo == 0u {
        return 0.0;
    }
    var top = firstLeadingBit(lo);
    if hi != 0u {
        top = 32u + firstLeadingBit(hi);
    }
    var m = lo;
    var e = -40;
    if top > 23u {
        // Keep 24 bits: shift right by s, and round by what falls off.
        let s = top - 23u;
        var up = false;
        if s < 32u {
            m = (lo >> s) | (hi << (32u - s));
            let rest = lo & ((1u << s) - 1u);
            let half = 1u << (s - 1u);
            up = rest > half || (rest == half && (m & 1u) == 1u);
        } else {
            let t = s - 32u;
            m = hi >> t;
            if t == 0u {
                up = lo > 0x80000000u || (lo == 0x80000000u && (m & 1u) == 1u);
            } else {
                let rest = hi & ((1u << t) - 1u);
                let half = 1u << (t - 1u);
                up = rest > half || (rest == half && (lo != 0u || (m & 1u) == 1u));
            }
        }
        e = i32(s) - 40;
        if up {
            m += 1u;
            if m == (1u << 24u) {
                m = 1u << 23u;
                e += 1;
            }
        }
    }
    let v = ldexp(f32(m), e);
    return select(v, -v, negative);
}

fn nearest4(a: Wide) -> vec4<f32> {
    return vec4(
        nearest(a.lo.x, a.hi.x),
        nearest(a.lo.y, a.hi.y),
        nearest(a.lo.z, a.hi.z),
        nearest(a.lo.w, a.hi.w),
    );
}

// ---- the running sums of every row ----

// Sums one chunk of one row exactly.
@compute @workgroup_size(64)
fn row_chunks(@builtin(global_invocation_id) gid: vec3<u32>) {
    let y = gid.x;
    let c = gid.y;
    let start = c * params.chunk;
    if y >= params.height || start >= params.width {
        return;
    }
    let end = min(start + params.chunk, params.width);
    var sum = Wide(vec4(0u), vec4(0u));
    for (var x = start; x < end; x++) {
        sum = wide_add(sum, fixed(grid[y * params.width + x]));
    }
    let at = 2u * (y * params.chunks + c);
    written[at] = sum.lo;
    written[at + 1u] = sum.hi;
}

// Each chunk's sum becomes the sum of the chunks before it on its row.
@compute @workgroup_size(64)
fn row_scan(@builtin(global_invocation_id) gid: vec3<u32>) {
    let y = gid.x;
    if y >= params.height {
        return;
    }
    var sum = Wide(vec4(0u), vec4(0u));
    for (var c = 0u; c < params.chunks; c++) {
        let at = 2u * (y * params.chunks + c);
        let s = Wide(written[at], written[at + 1u]);
        written[at] = sum.lo;
        written[at + 1u] = sum.hi;
        sum = wide_add(sum, s);
    }
}

// The running sums after every pixel of one chunk of one row, as floats.
@compute @workgroup_size(64)
fn row_sums(@builtin(global_invocation_id) gid: vec3<u32>) {
    let y = gid.x;
    let c = gid.y;
    let start = c * params.chunk;
    if y >= params.height || start >= params.width {
        return;
    }
    let end = min(start + params.chunk, params.width);
    let at = 2u * (y * params.chunks + c);
    var sum = Wide(chunk_sums[at], chunk_sums[at + 1u]);
    let base = y * (params.width + 1u);
    if c == 0u {
        written[base] = vec4(0u);
    }
    for (var x = start; x < end; x++) {
        sum = wide_add(sum, fixed(grid[y * params.width + x]));
        written[base + x + 1u] = bitcast<vec4<u32>>(nearest4(sum));
    }
}

// ---- a pixel's iris ----

// `RowSums::prefix`: the sum over row `y` from its left edge to `x`, the
// px `whole` and `part` past it, as a running sum and what is added to it,
// so a difference of two takes the running sums' difference first.
struct Prefix {
    sum: vec4<f32>,
    more: vec4<f32>,
}

fn prefix(y: u32, whole: i32, part: f32) -> Prefix {
    let w = i32(params.width);
    let base = y * (params.width + 1u);
    if whole < 0 || (whole == 0 && part <= 0.0) {
        if params.repeat == 0u {
            return Prefix(vec4(0.0), vec4(0.0));
        }
        return Prefix(vec4(0.0), grid[y * params.width] * (f32(whole) + part));
    }
    if whole >= w {
        let all = sums[base + params.width];
        if params.repeat == 0u {
            return Prefix(all, vec4(0.0));
        }
        return Prefix(all, grid[y * params.width + params.width - 1u] * (f32(whole - w) + part));
    }
    let a = sums[base + u32(whole)];
    let b = sums[base + u32(whole) + 1u];
    return Prefix(a, (b - a) * part);
}

// `v` as it is: the GPU may reorder float sums, and this keeps a
// difference of two running sums from being merged with the small parts
// added to them, which would round it by the running sums' size.
fn fenced(v: vec4<f32>) -> vec4<f32> {
    return bitcast<vec4<f32>>(bitcast<vec4<u32>>(v) ^ vec4(params.zero));
}

// `RowSums::sum`: the sum over row `y` between two places, pixel `x`'s
// left edge plus `whole0 + part0` and `whole1 + part1` px, times
// `share`; nothing past the top and bottom unless they repeat.
fn row_part(y: i32, x: u32, whole0: i32, part0: f32, whole1: i32, part1: f32, share: f32) -> vec4<f32> {
    var yy = y;
    if yy < 0 || yy >= i32(params.height) {
        if params.repeat == 0u {
            return vec4(0.0);
        }
        yy = clamp(yy, 0, i32(params.height) - 1);
    }
    let a = prefix(u32(yy), i32(x) + whole0, part0);
    let b = prefix(u32(yy), i32(x) + whole1, part1);
    return (fenced(b.sum - a.sum) + (b.more - a.more)) * share;
}

// A row's edge `off` px from a pixel's left edge, as a whole px and a part.
fn row_part_at(y: i32, x: u32, off0: f32, off1: f32, share: f32) -> vec4<f32> {
    let w0 = floor(off0);
    let w1 = floor(off1);
    return row_part(y, x, i32(w0), off0 - w0, i32(w1), off1 - w1, share);
}

// `Iris::at`: the iris's left and right at height `y`, at a radius of 1.
fn iris_at(y: f32) -> vec2<f32> {
    let t = clamp((y - params.top) / (params.bottom - params.top), 0.0, 1.0) * f32(IRIS_ROWS - 1u);
    let i = min(u32(floor(t)), IRIS_ROWS - 2u);
    let f = t - f32(i);
    let a = bitcast<vec2<f32>>(table[i].xy);
    let b = bitcast<vec2<f32>>(table[i + 1u].xy);
    return a + (b - a) * f;
}

// A float rounded half away from zero, as the CPU rounds.
fn round_away(v: f32) -> f32 {
    return sign(v) * floor(abs(v) + 0.5);
}

fn pixel_at(x: i32, y: i32) -> vec4<f32> {
    if x < 0 || y < 0 || x >= i32(params.map_width) || y >= i32(params.map_height) {
        return vec4(0.0);
    }
    return map[u32(y) * params.map_width + u32(x)];
}

// The radius the map gives pixel (x, y): where its centre reads the map,
// the map's level there (`Level::of`), and how far that is from focus.
fn mapped_radius(x: u32, y: u32) -> f32 {
    let steps = params.map_steps;
    let whole = steps.xy * i32(x) + steps.zw * i32(y);
    let unit = i32(1u << params.shift);
    let part = params.map_part.xy
        + vec2<f32>(whole & vec2(unit - 1)) / f32(unit)
        + params.map_rest.xy * f32(x) + params.map_rest.zw * f32(y);
    let w = floor(part);
    let at = params.map_whole.xy + (whole >> vec2(params.shift)) + vec2<i32>(w);
    let f = part - w;
    let a = pixel_at(at.x, at.y);
    let b = pixel_at(at.x + 1, at.y);
    let c = pixel_at(at.x, at.y + 1);
    let d = pixel_at(at.x + 1, at.y + 1);
    let top = a + (b - a) * f.x;
    let bottom = c + (d - c) * f.x;
    let p = top + (bottom - top) * f.y;
    var s = vec3(0.0);
    if p.w > 0.0 {
        s = p.rgb / p.w;
    }
    var l = 0.0;
    switch params.level {
        case 0u: {
            l = s.x * 0.299 + s.y * 0.587 + s.z * 0.114;
        }
        case 1u: {
            l = s.x;
        }
        case 2u: {
            l = s.y;
        }
        case 3u: {
            l = s.z;
        }
        default: {
            l = p.w;
        }
    }
    if params.inverted != 0u {
        l = 1.0 - l;
    }
    return params.radius * abs(l - params.focal);
}

// `spread`: every pixel the mean of the layer over its iris.
@compute @workgroup_size(256)
fn spread(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.y * params.row + gid.x;
    if i >= params.count {
        return;
    }
    let x = i % params.width;
    let y = i32(i / params.width);
    let own = grid[i];
    var r = params.radius;
    if params.mapped != 0u {
        r = mapped_radius(x, u32(y));
    }
    // Below a 20th of a px the iris is the pixel itself.
    if r < 0.05 {
        written[i] = bitcast<vec4<u32>>(own);
        return;
    }
    // Summed 64 rows at a time, then together, so a wide iris's thousands
    // of rows round no more than a few.
    var sum = vec4(0.0);
    var part = vec4(0.0);
    var weight = params.weight;
    var held = 0u;
    if params.mapped == 0u {
        for (var k = 0u; k < params.rows; k++) {
            let a = table[2u * k];
            let b = bitcast<vec2<f32>>(table[2u * k + 1u].xy);
            let w = bitcast<vec2<i32>>(a.yz);
            part += row_part(y + bitcast<i32>(a.x), x, w.x, b.x, w.y, b.y, bitcast<f32>(a.w));
            held += 1u;
            if held == 64u {
                sum += part;
                part = vec4(0.0);
                held = 0u;
            }
        }
    } else {
        // `Iris::rows` at this pixel's radius.
        let top = -params.bottom * r;
        let bottom = -params.top * r;
        let first = i32(round_away(top - 0.5));
        let last = i32(round_away(bottom + 0.5));
        weight = 0.0;
        var weight_part = 0.0;
        for (var dy = first; dy <= last; dy++) {
            let low = max(f32(dy) - 0.5, top);
            let high = min(f32(dy) + 0.5, bottom);
            if high <= low {
                continue;
            }
            let e = iris_at(-(low + high) / 2.0 / r);
            let share = high - low;
            let left = -e.y * r;
            let right = -e.x * r;
            part += row_part_at(y + dy, x, 0.5 + left, 0.5 + right, share);
            weight_part += share * (right - left);
            held += 1u;
            if held == 64u {
                sum += part;
                weight += weight_part;
                part = vec4(0.0);
                weight_part = 0.0;
                held = 0u;
            }
        }
        weight += weight_part;
    }
    sum += part;
    if weight <= 1e-9 {
        written[i] = bitcast<vec4<u32>>(own);
        return;
    }
    written[i] = bitcast<vec4<u32>>(sum / weight);
}
