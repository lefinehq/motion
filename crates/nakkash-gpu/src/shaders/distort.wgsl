// Distortions (`distort.rs`): every pixel redrawn from the point of the
// layer's own coordinates its effect gives for the layer point at its
// centre, sampled bilinearly from the four nearest pixels, transparent
// beyond the edges (`Pixels::sample`), or transparent where it gives none.
// Each kernel ports the CPU function named beside it.
//
// Points are worked out in the layer's coordinates in 32-bit floats, as
// the CPU works them out. Where the CPU then takes the point read to the
// grid in 64-bit floats, the kernels take only its offset from the layer
// point of the pixel's centre, `to`'s linear part times the difference,
// and add it to the pixel's whole column and row: at 4K a grid coordinate
// in 32 bits is a quarter of a thousandth of a px from where 64 put it,
// an offset of a few px a hundred times nearer.
//
// The layer point of each pixel's centre is the CPU's own: its 64-bit
// value is the sum of a part by column and a part by row (`table`), which
// are added here as fixed-point numbers and rounded to 32 bits as the CPU
// rounds it, keeping what the rounding moved it by. The CPU reads the
// grid that far from the pixel's centre, which decides, for one, the
// colour of a map read where a transparent pixel meets another.

struct Params {
    count: u32,
    row: u32,
    width: u32,
    height: u32,
    // The map's grid (`displacement-map`).
    map_width: u32,
    map_height: u32,
    // Choices: what each kernel names them.
    a: u32,
    b: u32,
    c: u32,
    d: u32,
    e: u32,
    f: u32,
    // Whether the layer points of the pixels' centres are in `table`.
    exact: u32,
    // From grid px to the layer's coordinates: x = a·px + c·py, y =
    // b·px + d·py, plus `shift`, the layer point of px (0, 0)'s centre.
    back: vec4<f32>,
    shift: vec4<f32>,
    // The linear part of the layer's transform to the grid, as `back`.
    to: vec4<f32>,
    // The box's left and top, width and height, in 32-bit floats as the
    // CPU rounds them.
    origin: vec4<f32>,
    // The box for `Rect::contains`: each side the least 32-bit float at or
    // past the 64-bit one, so the comparison is the CPU's exactly.
    inside: vec4<f32>,
    v0: vec4<f32>,
    v1: vec4<f32>,
    v2: vec4<f32>,
    v3: vec4<f32>,
    v4: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> map: array<vec4<f32>>;
// The layer point of each pixel's centre, in parts: for every column, the
// x and y of `back` applied to the column's centre and 0, then for every
// row those of 0 and the row's centre. Each is a 64-bit two's complement
// number of 2^-32 px, its high word first.
@group(0) @binding(4) var<storage, read> table: array<u32>;

// `std::f32::consts::TAU` and `SQRT_2`.
const TURN: f32 = 6.2831855;
const SQRT_2: f32 = 1.4142135;

// The pixel an invocation draws, its column and row, or none past the
// grid's end; the layer point at its centre, and how far the CPU's
// rounding of it moved it from there.
struct Pixel {
    i: u32,
    x: u32,
    y: u32,
    at: vec2<f32>,
    rounded: vec2<f32>,
}

fn pixel_of(gid: vec3<u32>) -> Pixel {
    let i = gid.y * params.row + gid.x;
    let width = max(params.width, 1u);
    let x = i % width;
    let y = i / width;
    let px = f32(x);
    let py = f32(y);
    if params.exact != 0u && i < params.count {
        let c = 4u * x;
        let r = 4u * (params.width + y);
        let ax = rounded(sum(table[c], table[c + 1u], table[r], table[r + 1u]));
        let ay = rounded(sum(table[c + 2u], table[c + 3u], table[r + 2u], table[r + 3u]));
        return Pixel(i, x, y, vec2(ax.x, ay.x), vec2(ax.y, ay.y));
    }
    // `remap`: the layer point at the pixel's centre.
    let at = vec2(
        params.shift.x + params.back.x * px + params.back.z * py,
        params.shift.y + params.back.y * px + params.back.w * py,
    );
    return Pixel(i, x, y, at, vec2(0.0));
}

// The sum of two 64-bit numbers, each its high and low words.
fn sum(ah: u32, al: u32, bh: u32, bl: u32) -> vec2<u32> {
    let low = al + bl;
    return vec2(ah + bh + select(0u, 1u, low < al), low);
}

// A 64-bit two's complement number of 2^-32 units, its high and low words,
// rounded to the nearest 32-bit float, ties to even, as Rust's `as f32`
// rounds a 64-bit float; and what the rounding added to it. Its magnitude
// stays under 2^22, so at most 31 bits are rounded off.
fn rounded(v: vec2<u32>) -> vec2<f32> {
    let negative = (v.x >> 31u) != 0u;
    var high = v.x;
    var low = v.y;
    if negative {
        low = ~v.y + 1u;
        high = ~v.x + select(0u, 1u, low == 0u);
    }
    if high == 0u && low == 0u {
        return vec2(0.0);
    }
    // The place of its highest bit.
    var top = 31u - countLeadingZeros(low);
    if high != 0u {
        top = 63u - countLeadingZeros(high);
    }
    var mantissa = 0u;
    // What rounding added to the magnitude, in units.
    var added = 0.0;
    if top <= 23u {
        mantissa = low << (23u - top);
    } else {
        let k = top - 23u;
        mantissa = (high << (32u - k)) | (low >> k);
        let dropped = low & ((1u << k) - 1u);
        let half = 1u << (k - 1u);
        if dropped > half || (dropped == half && (mantissa & 1u) == 1u) {
            mantissa = mantissa + 1u;
            added = f32((1u << k) - dropped);
        } else {
            added = -f32(dropped);
        }
    }
    // 2^(top - 32), biased.
    var exponent = top + 95u;
    if mantissa == 0x1000000u {
        mantissa = 0x800000u;
        exponent = exponent + 1u;
    }
    let sign = select(0u, 0x80000000u, negative);
    let value = bitcast<f32>(sign | (exponent << 23u) | (mantissa & 0x7FFFFFu));
    var moved = ldexp(added, -32);
    if negative {
        moved = -moved;
    }
    return vec2(value, moved);
}

// `to`'s linear part applied to `d`, a difference of layer points.
fn on_grid(d: vec2<f32>) -> vec2<f32> {
    return vec2(params.to.x * d.x + params.to.z * d.y, params.to.y * d.x + params.to.w * d.y);
}

// `Rect::contains` for the box `r`, x0, y0, x1, y1.
fn holds(r: vec4<f32>, p: vec2<f32>) -> bool {
    return p.x >= r.x && p.x < r.z && p.y >= r.y && p.y < r.w;
}

// `f32::rem_euclid` for `b` above 0: `a` less the multiple of `b` at or
// below it, rounded once, as the remainder of a division and the sum after
// it round.
fn wrapped(a: f32, b: f32) -> f32 {
    let n = floor(a / b);
    var r = fma(-n, b, a);
    // The quotient rounded across a whole number.
    if r < 0.0 {
        r = r + b;
    } else if r >= b {
        r = r - b;
    }
    return r;
}

// `Pixels::get` of the layer's pixels.
fn src_at(x: i32, y: i32) -> vec4<f32> {
    if x < 0 || y < 0 || x >= i32(params.width) || y >= i32(params.height) {
        return vec4(0.0);
    }
    return src[u32(y) * params.width + u32(x)];
}

// `Pixels::get` of the map's.
fn map_at(x: i32, y: i32) -> vec4<f32> {
    if x < 0 || y < 0 || x >= i32(params.map_width) || y >= i32(params.map_height) {
        return vec4(0.0);
    }
    return map[u32(y) * params.map_width + u32(x)];
}

fn lerp(a: vec4<f32>, b: vec4<f32>, t: f32) -> vec4<f32> {
    return a + (b - a) * t;
}

// A grid point `o` px from the centre of the pixel (`x`, `y`): the whole
// px it falls on and how far past it, as `Pixels::sample` splits it.
struct Split {
    x0: i32,
    y0: i32,
    fx: f32,
    fy: f32,
}

fn split(x: u32, y: u32, o: vec2<f32>) -> Split {
    // Far past any grid, so whole px stay within 32 bits.
    let held = clamp(o, vec2(-1e9), vec2(1e9));
    let whole = floor(held);
    return Split(
        i32(x) + i32(whole.x),
        i32(y) + i32(whole.y),
        held.x - whole.x,
        held.y - whole.y,
    );
}

// `Pixels::sample` of the layer's pixels at `o` px from the pixel's centre.
fn sample_src(x: u32, y: u32, o: vec2<f32>) -> vec4<f32> {
    let s = split(x, y, o);
    let top = lerp(src_at(s.x0, s.y0), src_at(s.x0 + 1, s.y0), s.fx);
    let bottom = lerp(src_at(s.x0, s.y0 + 1), src_at(s.x0 + 1, s.y0 + 1), s.fx);
    return lerp(top, bottom, s.fy);
}

// `Pixels::sample` of the map's.
fn sample_map(x: u32, y: u32, o: vec2<f32>) -> vec4<f32> {
    let s = split(x, y, o);
    let top = lerp(map_at(s.x0, s.y0), map_at(s.x0 + 1, s.y0), s.fx);
    let bottom = lerp(map_at(s.x0, s.y0 + 1), map_at(s.x0 + 1, s.y0 + 1), s.fx);
    return lerp(top, bottom, s.fy);
}

// The layer's pixels at its point `source`, for the pixel `p`.
fn read_at(p: Pixel, source: vec2<f32>) -> vec4<f32> {
    return sample_src(p.x, p.y, on_grid(source - p.at + p.rounded));
}

// The layer's pixels `offset` from the layer point of `p`'s centre.
fn read_by(p: Pixel, offset: vec2<f32>) -> vec4<f32> {
    return sample_src(p.x, p.y, on_grid(offset + p.rounded));
}

// `f32::atan2`, with its signs of zero: a fast arctangent may take an x
// of -0 for +0.
fn angle_of(y: f32, x: f32) -> f32 {
    if x == 0.0 {
        if y > 0.0 {
            return TURN / 4.0;
        }
        if y < 0.0 {
            return -TURN / 4.0;
        }
        // Both zero: ±0 for an x of +0, ±π for -0, the sign y's.
        var a = 0.0;
        if (bitcast<u32>(x) >> 31u) != 0u {
            a = TURN / 2.0;
        }
        if (bitcast<u32>(y) >> 31u) != 0u {
            a = -a;
        }
        return a;
    }
    return atan2(y, x);
}

// `motion_tile`. v0: centre, phase; v1: tile size, output size; v2: the
// box's middle. a: mirror; b: the phase shifts rows.
@compute @workgroup_size(256)
fn motion_tile(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = pixel_of(gid);
    if p.i >= params.count {
        return;
    }
    let x = p.at.x;
    let y = p.at.y;
    let center = params.v0.xy;
    let phase = params.v0.z;
    let tile = params.v1.xy;
    let output = params.v1.zw;
    let middle = params.v2.xy;
    if abs(x - middle.x) > output.x / 2.0 || abs(y - middle.y) > output.y / 2.0 {
        dst[p.i] = vec4(0.0);
        return;
    }
    var u = vec2((x - center.x) / tile.x + 0.5, (y - center.y) / tile.y + 0.5);
    // Every other column (or row) shifts by the phase.
    if params.b != 0u {
        if wrapped(floor(u.y), 2.0) == 1.0 {
            u.x = u.x + phase;
        }
    } else if wrapped(floor(u.x), 2.0) == 1.0 {
        u.y = u.y + phase;
    }
    var at = vec2(0.0);
    for (var k = 0; k < 2; k++) {
        let f = wrapped(u[k], 1.0);
        let flipped = params.a != 0u && wrapped(floor(u[k]), 2.0) == 1.0;
        var g = f;
        if flipped {
            g = 1.0 - f;
        }
        at[k] = params.origin[k] + g * params.origin[k + 2];
    }
    dst[p.i] = read_at(p, at);
}

// `wrap`. v0: the shift, the share of the layer kept.
@compute @workgroup_size(256)
fn wrap(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = pixel_of(gid);
    if p.i >= params.count {
        return;
    }
    let at = p.at - params.v0.xy;
    var source = at;
    for (var k = 0; k < 2; k++) {
        let size = params.origin[k + 2];
        if size > 0.0 {
            source[k] = params.origin[k] + wrapped(at[k] - params.origin[k], size);
        }
    }
    var out = read_at(p, source);
    // `Pixels::mix` towards the layer as it was.
    let blend = params.v0.z;
    if blend > 0.0 {
        let t = clamp(blend, 0.0, 1.0);
        out = out + (src[p.i] - out) * t;
    }
    dst[p.i] = out;
}

// `mirror`. v0: the line's point, the sine and cosine of its normal.
@compute @workgroup_size(256)
fn mirror(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = pixel_of(gid);
    if p.i >= params.count {
        return;
    }
    if !holds(params.inside, p.at) {
        dst[p.i] = vec4(0.0);
        return;
    }
    let center = params.v0.xy;
    let sine = params.v0.z;
    let cosine = params.v0.w;
    let x = p.at.x;
    let y = p.at.y;
    // How far the point lies on the reflected side of the line.
    let beyond = (x - center.x) * cosine + (y - center.y) * sine;
    var source = p.at;
    if beyond > 0.0 {
        source = vec2(x - 2.0 * beyond * cosine, y - 2.0 * beyond * sine);
    }
    dst[p.i] = read_at(p, source);
}

// `polar_coordinates`. v0: the box's left, top, width and height; v1: its
// middle, the share of the way each pixel goes. a: rings unroll.
@compute @workgroup_size(256)
fn polar_coordinates(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = pixel_of(gid);
    if p.i >= params.count {
        return;
    }
    if !holds(params.inside, p.at) {
        dst[p.i] = vec4(0.0);
        return;
    }
    let left = params.v0.x;
    let top = params.v0.y;
    let width = params.v0.z;
    let middle = params.v1.xy;
    let amount = params.v1.z;
    let x = p.at.x;
    let y = p.at.y;
    var converted: vec2<f32>;
    if params.a == 0u {
        let dx = x - middle.x;
        let dy = y - middle.y;
        let angle = wrapped(angle_of(-dx, -dy), TURN);
        converted = vec2(left + width * angle / TURN, top + SQRT_2 * sqrt(dx * dx + dy * dy));
    } else {
        let angle = TURN * (x - left) / max(width, 1e-3);
        let radius = (y - top) / SQRT_2;
        converted = vec2(middle.x - radius * sin(angle), middle.y - radius * cos(angle));
    }
    let source = vec2(x + (converted.x - x) * amount, y + (converted.y - y) * amount);
    dst[p.i] = read_at(p, source);
}

// `lens_distortion`. v0: the lens's centre, its focal length; v1: the box
// it draws in, as `inside`; `inside` is the box it reads. a: the lens is
// removed.
@compute @workgroup_size(256)
fn lens_distortion(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = pixel_of(gid);
    if p.i >= params.count {
        return;
    }
    if !holds(params.v1, p.at) {
        dst[p.i] = vec4(0.0);
        return;
    }
    let d = p.at - params.v0.xy;
    let focal = params.v0.z;
    let r = sqrt(d.x * d.x + d.y * d.y);
    // `Lens::read`, as the factor the point's offset from the axis grows
    // by, less 1: written so it keeps its digits where it is near 0.
    var more = 0.0;
    if r > 0.0 {
        let q = r / focal;
        if params.a != 0u {
            // `bent`: r / √(1 + q²).
            let s = sqrt(1.0 + q * q);
            more = -(q * q) / (s * (1.0 + s));
        } else {
            // `unbent`: r / √(1 − q²), nothing from the focal length out.
            let left = (1.0 - q) * (1.0 + q);
            if !(left > 0.0) {
                dst[p.i] = vec4(0.0);
                return;
            }
            let t = sqrt(left);
            more = (q * q) / (t * (1.0 + t));
        }
    }
    let offset = d * more;
    if !holds(params.inside, p.at + offset) {
        dst[p.i] = vec4(0.0);
        return;
    }
    dst[p.i] = read_by(p, offset);
}

// `repeat_tile`. v0: how far the tiles reach, as `inside`. a: the tiling,
// 0 repeat, 1 unfold, 2 brick, 3 none.
@compute @workgroup_size(256)
fn repeat_tile(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = pixel_of(gid);
    if p.i >= params.count {
        return;
    }
    if !holds(params.v0, p.at) {
        dst[p.i] = vec4(0.0);
        return;
    }
    let x = p.at.x;
    let y = p.at.y;
    let origin = params.origin.xy;
    let size = params.origin.zw;
    let tiling = params.a;
    let row = floor((y - origin.y) / size.y);
    if tiling == 3u && !holds(params.inside, p.at) {
        dst[p.i] = vec4(0.0);
        return;
    }
    var shift = 0.0;
    if tiling == 2u && wrapped(row, 2.0) == 1.0 {
        shift = size.x / 2.0;
    }
    let column = floor((x - origin.x - shift) / size.x);
    let along = vec2(x - origin.x - shift - column * size.x, y - origin.y - row * size.y);
    let n = vec2(column, row);
    var source = vec2(0.0);
    for (var k = 0; k < 2; k++) {
        if tiling == 1u && wrapped(n[k], 2.0) == 1.0 {
            source[k] = origin[k] + (size[k] - along[k]);
        } else {
            source[k] = origin[k] + along[k];
        }
    }
    dst[p.i] = read_at(p, source);
}

// `generate::lattice`: a lattice point's value, 0 to 1.
fn lattice(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    var h = (seed * 0x9E3779B9u) ^ (bitcast<u32>(x) * 0x85EBCA6Bu)
        ^ (bitcast<u32>(y) * 0xC2B2AE35u) ^ (bitcast<u32>(z) * 0x27D4EB2Fu);
    h = h ^ (h >> 15u);
    h = h * 0x2C1B3C6Du;
    h = h ^ (h >> 12u);
    h = h * 0x297A2D39u;
    h = h ^ (h >> 15u);
    return f32(h >> 8u) / 16777216.0;
}

fn ease(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

fn mixed(a: f32, b: f32, t: f32) -> f32 {
    return a + (b - a) * t;
}

// `generate::value_noise` of the spline kind, 0 to 1.
fn value_noise(p: vec3<f32>, seed: u32) -> f32 {
    let cell = floor(p);
    let f = p - cell;
    let at = vec3<i32>(cell);
    let w = vec3(ease(f.x), ease(f.y), ease(f.z));
    var plane: array<f32, 2>;
    for (var dz = 0; dz < 2; dz++) {
        let z = at.z + dz;
        plane[dz] = mixed(
            mixed(lattice(at.x, at.y, z, seed), lattice(at.x + 1, at.y, z, seed), w.x),
            mixed(lattice(at.x, at.y + 1, z, seed), lattice(at.x + 1, at.y + 1, z, seed), w.x),
            w.y,
        );
    }
    return mixed(plane[0], plane[1], w.z);
}

// `turbulent_displace`'s field: octaves of noise summed, -1 to 1. v0: the
// noise's offset, its cell, its complexity; v1: its evolution. b: how many
// octaves; c: the finer octaves weigh half.
fn field(p: vec2<f32>, seed: u32) -> f32 {
    let offset = params.v0.xy;
    let size = params.v0.z;
    let complexity = params.v0.w;
    var finer = 1.0;
    if params.c != 0u {
        finer = 0.5;
    }
    var sum = 0.0;
    var total = 0.0;
    var weight = 1.0;
    var scale = 1.0;
    for (var octave = 0u; octave < params.b; octave++) {
        let share = min(complexity - f32(octave), 1.0) * finer;
        let at = vec3((p.x - offset.x) / size * scale, (p.y - offset.y) / size * scale, params.v1.x);
        sum = sum + (value_noise(at, seed + octave) - 0.5) * weight * share;
        total = total + weight * share;
        weight = weight * 0.5;
        scale = scale * 2.0;
    }
    if total > 0.0 {
        return 2.0 * sum / total;
    }
    return 0.0;
}

// The slope of the noise across a cell, each way.
fn slope(p: vec2<f32>, seed: u32) -> vec2<f32> {
    let size = params.v0.z;
    let e = size / 8.0;
    let ex = field(vec2(p.x + e, p.y), seed) - field(vec2(p.x - e, p.y), seed);
    let ey = field(vec2(p.x, p.y + e), seed) - field(vec2(p.x, p.y - e), seed);
    return vec2(ex / (2.0 * e) * size / 4.0, ey / (2.0 * e) * size / 4.0);
}

// `turbulent_displace`. v1: evolution, how far the most moves (`reach`);
// v2: the box, x0, y0, x1, y1. a: the displacement, in the order of its
// choices; d: the seed; e: the seed along y; f: the pinning, 0 all, 1 left
// and right, 2 top and bottom, 3 none.
@compute @workgroup_size(256)
fn turbulent_displace(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = pixel_of(gid);
    if p.i >= params.count {
        return;
    }
    let at = p.at;
    let size = params.v0.z;
    let seed = params.d;
    // How much of the displacement each axis keeps near pinned edges.
    let b = params.v2;
    let edges = vec2(
        min(clamp((at.x - b.x) / size, 0.0, 1.0), clamp((b.z - at.x) / size, 0.0, 1.0)),
        min(clamp((at.y - b.y) / size, 0.0, 1.0), clamp((b.w - at.y) / size, 0.0, 1.0)),
    );
    let every = min(edges.x, edges.y);
    var keep = vec2(1.0);
    switch params.f {
        case 0u: {
            keep = vec2(every);
        }
        case 1u: {
            keep = vec2(edges.x);
        }
        case 2u: {
            keep = vec2(edges.y);
        }
        default: {}
    }
    var d: vec2<f32>;
    switch params.a {
        // Bulges: out along the noise's slope.
        case 2u, 3u: {
            d = slope(at, seed);
        }
        // Twists: across it, round its hills.
        case 4u, 5u: {
            let s = slope(at, seed);
            d = vec2(-s.y, s.x);
        }
        case 6u: {
            d = vec2(field(at, seed), 0.0);
        }
        case 7u: {
            d = vec2(0.0, field(at, seed));
        }
        case 8u: {
            let n = field(at, seed);
            d = vec2(n);
        }
        default: {
            d = vec2(field(at, seed), field(at, params.e));
        }
    }
    let reach = params.v1.y;
    let offset = vec2(d.x * reach * keep.x, d.y * reach * keep.y);
    dst[p.i] = read_by(p, offset);
}

// `Level::of` the map's straight colour, for a level 0 to 7 of
// `MapLevel`; 8 to 10 are the constants. v3: the luma's weights.
fn level(which: u32, pixel: vec4<f32>) -> f32 {
    switch which {
        case 8u: {
            return 1.0;
        }
        case 9u: {
            return 0.75;
        }
        case 10u: {
            return 0.5;
        }
        default: {}
    }
    var c = vec3(0.0);
    if pixel.a > 0.0 {
        c = vec3(pixel.r / pixel.a, pixel.g / pixel.a, pixel.b / pixel.a);
    }
    let r = c.x;
    let g = c.y;
    let b = c.z;
    let most = max(max(r, g), b);
    let least = min(min(r, g), b);
    var l = 0.0;
    switch which {
        case 0u: {
            l = r;
        }
        case 1u: {
            l = g;
        }
        case 2u: {
            l = b;
        }
        case 3u: {
            l = pixel.a;
        }
        case 4u: {
            l = r * params.v3.x + g * params.v3.y + b * params.v3.z;
        }
        case 5u: {
            let d = most - least;
            if d <= 0.0 {
                l = 0.0;
            } else if most == r {
                l = wrapped((g - b) / d, 6.0) / 6.0;
            } else if most == g {
                l = ((b - r) / d + 2.0) / 6.0;
            } else {
                l = ((r - g) / d + 4.0) / 6.0;
            }
        }
        case 6u: {
            l = (most + least) / 2.0;
        }
        default: {
            let ll = (most + least) / 2.0;
            let d = most - least;
            if d <= 0.0 {
                l = 0.0;
            } else {
                l = d / max(1.0 - abs(2.0 * ll - 1.0), 1e-6);
            }
        }
    }
    return clamp(l, 0.0, 1.0);
}

// `displacement_map`. v0: the most each way, the map's offset in grid px
// where it lies centred; v1: the layer box's left and top in 32-bit
// floats, and how much more than the layer's box the map's is wide and
// high (stretched); v2: the offset of the map's point from the layer's at
// that left and top (stretched), and the map's width and height (tiled);
// v4: the layer box's middle less the map's, plus the map's left and top,
// and the map's middle less the layer box's (tiled). a: the level along x; b: along y;
// c: the map lies centred (0), stretched (1) or tiled (2); d: the moves
// wrap round the box.
@compute @workgroup_size(256)
fn displacement_map(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = pixel_of(gid);
    if p.i >= params.count {
        return;
    }
    let x = p.at;
    // The map's point for the layer's, as an offset from it (`Lay::point`).
    var shift = vec2(0.0);
    switch params.c {
        case 1u: {
            shift = on_grid(params.v2.xy + (x - params.v1.xy) * params.v1.zw);
        }
        case 2u: {
            let q = x - params.v4.xy;
            var m = q;
            for (var k = 0; k < 2; k++) {
                m[k] = wrapped(q[k], params.v2[k + 2]);
            }
            shift = on_grid(m - q + params.v4.zw);
        }
        default: {
            shift = params.v0.zw;
        }
    }
    let pixel = sample_map(p.x, p.y, shift + on_grid(p.rounded));
    let most = params.v0.xy;
    let d = vec2(
        (level(params.a, pixel) - 0.5) * 2.0 * most.x,
        (level(params.b, pixel) - 0.5) * 2.0 * most.y,
    );
    var source = x + d;
    if params.d != 0u {
        for (var k = 0; k < 2; k++) {
            let size = params.origin[k + 2];
            if size > 0.0 {
                source[k] = params.origin[k] + wrapped(source[k] - params.origin[k], size);
            }
        }
    }
    dst[p.i] = read_at(p, source);
}
