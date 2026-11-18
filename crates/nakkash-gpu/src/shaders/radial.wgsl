// The radial blurs (`radial.rs`): every pixel the mean of the layer along
// a stretch of its ray out from a centre (a zoom) or of its ring round it
// (a spin), from the running sums of the layer read along each line.
//
// The CPU sums the samples of a line in 64-bit floats. Here they are
// summed as 64-bit fixed-point numbers of 2^-24, two 32-bit words a
// channel, which integer arithmetic adds exactly in any order: a line is
// read in chunks at once, and the sum over any stretch is the exact
// difference of two running sums, turned into a float only at the end.
//
// Where a line is read and where a pixel lies on it needs more than the 24
// bits of a float far from the centre: a whole px and a part of one are
// kept apart, and products that must be exact are of numbers with few
// enough bits. So a sample is read within a few 10^-7 px of where the CPU
// reads it, at any distance a 4K grid holds.
//
// Four steps run for each batch of lines: `*_keys` once, which line each
// pixel lies past; `*_lines`, the samples of the batch's lines and the
// sums of their chunks; `sums`, each chunk's sums turned into the sums of
// the chunks before it; `prefix`, the running sums of every sample; and
// `*_pixels`, the mean of every pixel whose lines are in the batch.

struct Params {
    // The grid's width and height, its pixels, and how many invocations
    // one row of workgroups covers.
    width: u32,
    height: u32,
    count: u32,
    row: u32,
    // How many spaces between lines the pixels fall in (`Rays::count`,
    // `Rings::count`), and a zoom's samples a ray.
    lines: u32,
    samples: u32,
    // The batch: its first line, and how many lines it reads, one more
    // than the spaces it covers.
    batch_first: u32,
    batch_lines: u32,
    // Samples a chunk, and the most chunks a line of the batch has.
    chunk: u32,
    chunks: u32,
    // 1 when the lines go all the way round; for a linear fade; for a
    // stretch across the pixel rather than behind it.
    whole: u32,
    linear: u32,
    across: u32,
    // How many vec4s a point of a line takes: the sample and the sum, and
    // the moment for a linear fade.
    stride: u32,
    // Where the chunks' sums start in the lines' buffer, in vec4s.
    chunk_base: u32,
    // A spin's innermost radius.
    inner: u32,
    // Where the batch's first ring starts among every ring's points and
    // chunks.
    point_base: u32,
    chunk_first: u32,
    spin: u32,
    // A zoom's ray steps are whole numbers over 2^shift, and a rest.
    shift: u32,
    // The centre's whole px, x and y, and what it has past them.
    centre_whole: vec4<i32>,
    centre_part: vec4<f32>,
    // A zoom's first ray's angle, the angle between rays and its first
    // sample's distance; the reach: a zoom's share of the distance, a
    // spin's angle in radians.
    view: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> written: array<vec4<u32>>;
// The layer's pixels.
@group(0) @binding(2) var<storage, read> grid: array<vec4<f32>>;
// A zoom's rays (three vec4s each) or a spin's rings (two each).
@group(0) @binding(3) var<storage, read> table: array<vec4<u32>>;
// The cosine and sine of 2^14 angles round a turn, each as three floats
// whose sum it is: two of few bits, and the small rest.
@group(0) @binding(4) var<storage, read> trig: array<vec4<f32>>;
// The batch's lines.
@group(0) @binding(5) var<storage, read> lines: array<vec4<u32>>;
// Which line each pixel lies past, four a vec4.
@group(0) @binding(6) var<storage, read> keys: array<vec4<u32>>;

const TAU: f32 = 6.283185307179586;
const TRIG: u32 = 16384u;
// 2^24 and 2^-24: the fixed-point numbers' scale.
const FIXED: f32 = 16777216.0;
const UNFIXED: f32 = 5.9604644775390625e-8;

// ---- 64-bit integers, four channels at once ----

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

fn wide_zero() -> Wide {
    return Wide(vec4(0u), vec4(0u));
}

fn wide_of(v: vec4<i32>) -> Wide {
    return Wide(bitcast<vec4<u32>>(v), select(vec4(0u), vec4(0xffffffffu), v < vec4(0)));
}

// The whole 64-bit product of two 32-bit unsigned numbers.
fn wide_product(a: vec4<u32>, b: vec4<u32>) -> Wide {
    let a0 = a & vec4(0xffffu);
    let a1 = a >> vec4(16u);
    let b0 = b & vec4(0xffffu);
    let b1 = b >> vec4(16u);
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;
    let mid = (p00 >> vec4(16u)) + (p01 & vec4(0xffffu)) + (p10 & vec4(0xffffu));
    let lo = (p00 & vec4(0xffffu)) | (mid << vec4(16u));
    let hi = p11 + (p01 >> vec4(16u)) + (p10 >> vec4(16u)) + (mid >> vec4(16u));
    return Wide(lo, hi);
}

// `a` times `m`, for |m| below 2^31.
fn wide_times(a: Wide, m: i32) -> Wide {
    var x = a;
    var k = m;
    if m < 0 {
        x = wide_sub(wide_zero(), a);
        k = -m;
    }
    let ku = vec4(u32(k));
    let p = wide_product(x.lo, ku);
    return Wide(p.lo, p.hi + x.hi * ku);
}

// A fixed-point number as a float.
fn wide_float(a: Wide) -> vec4<f32> {
    let small = (a.hi == vec4(0u) & a.lo < vec4(0x80000000u))
        | (a.hi == vec4(0xffffffffu) & a.lo >= vec4(0x80000000u));
    let exact = vec4<f32>(bitcast<vec4<i32>>(a.lo));
    let big = vec4<f32>(bitcast<vec4<i32>>(a.hi)) * 4294967296.0 + vec4<f32>(a.lo);
    return select(big, exact, small) * UNFIXED;
}

// A sample as a fixed-point number: exact to 2^-25, for values below 128.
fn fixed(x: vec4<f32>) -> vec4<i32> {
    return vec4<i32>(round(clamp(x, vec4(-127.99), vec4(127.99)) * FIXED));
}

// ---- points of the grid, a whole px and a part apart ----

// A point less half a px, as `Pixels::sample` takes it: the px it starts
// in, and how far into it, 0 to 1.
struct Point {
    whole: vec2<i32>,
    part: vec2<f32>,
}

fn point(whole: vec2<i32>, part: vec2<f32>) -> Point {
    let w = floor(part);
    return Point(whole + vec2<i32>(w), part - w);
}

fn pixel_at(x: i32, y: i32) -> vec4<f32> {
    if x < 0 || y < 0 || x >= i32(params.width) || y >= i32(params.height) {
        return vec4(0.0);
    }
    return grid[u32(y) * params.width + u32(x)];
}

// `Pixels::sample`: the four pixels round the point, blended.
fn sample_at(p: Point) -> vec4<f32> {
    let x = p.whole.x;
    let y = p.whole.y;
    let a = pixel_at(x, y);
    let b = pixel_at(x + 1, y);
    let c = pixel_at(x, y + 1);
    let d = pixel_at(x + 1, y + 1);
    let top = a + (b - a) * p.part.x;
    let bottom = c + (d - c) * p.part.x;
    return top + (bottom - top) * p.part.y;
}

// The pixel (x, y)'s centre less the point `p`, which lies near it.
fn from_point(x: u32, y: u32, p: Point) -> vec2<f32> {
    return vec2<f32>(vec2(i32(x), i32(y)) - p.whole) - p.part;
}

// The angle of (along, across) a line, for a small `across`: a series
// exact to the float where the builtin may round by a few px's worth far
// from the centre.
fn small_angle(across: f32, along: f32) -> f32 {
    if along > 0.0 && abs(across) < 0.25 * along {
        let z = across / along;
        let z2 = z * z;
        return z * (1.0 - z2 * (1.0 / 3.0 - z2 * (0.2 - z2 * (1.0 / 7.0 - z2 * (1.0 / 9.0
            - z2 * (1.0 / 11.0 - z2 * (1.0 / 13.0)))))));
    }
    return atan2(across, along);
}

// Where a pixel lies from a point of a line `radius` from the centre,
// `w` from it, the line running along `dir` there: how far it is from the
// centre past `radius`, and the angle from the point to it.
struct Offset {
    out: f32,
    turn: f32,
}

fn offset_from(w: vec2<f32>, dir: vec2<f32>, radius: f32) -> Offset {
    let a = dot(w, dir);
    let b = w.x * dir.y - w.y * dir.x;
    let r = sqrt((radius + a) * (radius + a) + b * b);
    var out = 0.0;
    if r + radius > 0.0 {
        out = (2.0 * radius * a + a * a + b * b) / (r + radius);
    }
    return Offset(out, small_angle(b, radius + a));
}

// ---- a zoom's rays ----

fn ray_direction(j: u32) -> vec2<f32> {
    return bitcast<vec2<f32>>(table[3u * j + 2u].xy);
}

// Sample `k` of ray `j`: the ray's first sample and `k` steps along it.
// The step is a whole number over 2^shift and a small rest, so its whole
// px and part are worked out in integers and the floats only add parts
// of a px: the GPU may reorder float sums, and these stay exact whatever
// the order.
fn ray_point(j: u32, k: u32) -> Point {
    let base = table[3u * j];
    let step = table[3u * j + 1u];
    let hi = bitcast<vec2<i32>>(step.xy) * i32(k);
    let unit = i32(1u << params.shift);
    let part = bitcast<vec2<f32>>(base.zw)
        + vec2<f32>(hi & vec2(unit - 1)) / f32(unit)
        + bitcast<vec2<f32>>(step.zw) * f32(k);
    return point(bitcast<vec2<i32>>(base.xy) + (hi >> vec2(params.shift)), part);
}

// Where pixel (x, y) lies on the rays: the space `j` it is past, the
// sample `k` nearest it on ray `j`, how far between rays `j` and `j + 1`
// (`t`), and how far from the centre past sample `k`.
struct OnRay {
    j: u32,
    k: u32,
    t: f32,
    out: f32,
}

fn on_ray(x: u32, y: u32, start: u32, settle: bool) -> OnRay {
    let first = params.view.z;
    let step = params.view.y;
    let last = i32(params.samples) - 1;
    let c = vec2<f32>(vec2(i32(x), i32(y)) - params.centre_whole.xy) + 0.5 - params.centre_part.xy;
    var k = clamp(i32(round(length(c) - first)), 0, last);
    var j = start;
    var t = 0.0;
    var out = 0.0;
    for (var round_ = 0; round_ < 8; round_++) {
        let p = ray_point(j, u32(k));
        let dir = ray_direction(j);
        let w = from_point(x, y, p);
        let radius = first + f32(k);
        let o = offset_from(w, dir, radius);
        t = o.turn / step;
        out = o.out;
        let k_next = clamp(k + i32(round(dot(w, dir))), 0, last);
        var j_next = i32(j);
        if settle && (t < 0.0 || t >= 1.0) {
            j_next = i32(j) + i32(floor(t));
            if params.whole != 0u {
                j_next = ((j_next % i32(params.lines)) + i32(params.lines)) % i32(params.lines);
            } else {
                j_next = clamp(j_next, 0, i32(params.lines) - 1);
            }
        }
        // A pixel on a tie may step back and forth: it keeps the last
        // place it was measured from.
        if (k_next == k && j_next == i32(j)) || round_ == 7 {
            break;
        }
        k = k_next;
        j = u32(j_next);
    }
    return OnRay(j, u32(k), clamp(t, 0.0, 1.0), out);
}

// ---- a spin's rings ----

struct Ring {
    // How many samples round it, its first sample's coordinate, how many
    // samples it takes, and where its points start in the batch.
    round: u32,
    first: i32,
    samples: u32,
    points: u32,
    // Where its chunks start in the batch, and its radius.
    chunks: u32,
    radius: u32,
}

fn ring(k: u32) -> Ring {
    let a = table[2u * k];
    let b = table[2u * k + 1u];
    return Ring(a.x, bitcast<i32>(a.y), a.z, a.w - params.point_base, b.x - params.chunk_first, b.y);
}

// `s` taken round to 0 to `n - 1`.
fn wrapped(s: i32, n: u32) -> u32 {
    let m = i32(n);
    return u32(((s % m) + m) % m);
}

// Sample `s` (0 to `round - 1`) of a ring: at 2π s / round, from the table
// of angles and the small turn past the nearest below, whose cosine and
// sine a series gives. The table holds each cosine and sine as a whole
// number over 2^15 and a rest: the radius times the whole number is
// worked out in integers, so its whole px and part are exact, and the
// floats only add parts of a px.
struct RingPoint {
    at: Point,
    dir: vec2<f32>,
}

// `radius` times `m` / 2^15: its whole px, and the part as a float.
struct Scaled {
    whole: i32,
    part: f32,
}

fn scaled(radius: u32, m: i32) -> Scaled {
    let hi = i32(radius >> 8u) * m;
    let lo = i32(radius & 255u) * m;
    return Scaled((hi >> 7u) + (lo >> 15u), f32(hi & 127) / 128.0 + f32(lo & 32767) / 32768.0);
}

fn ring_point(radius: u32, n: u32, s: u32) -> RingPoint {
    // s 2^14 = m n + rest, in two steps of 2^7 so nothing overflows.
    let t1 = s * 128u;
    let m1 = t1 / n;
    let t2 = (t1 % n) * 128u;
    let m = m1 * 128u + t2 / n;
    let rest = t2 % n;
    let e = (TAU / f32(TRIG)) * (f32(rest) / f32(n));
    let sin_e = e - e * e * e / 6.0;
    let cos_e = -0.5 * e * e;
    let entry = trig[m];
    let whole_cs = bitcast<vec2<i32>>(entry.xy);
    let cs = vec2<f32>(whole_cs) / 32768.0 + entry.zw;
    let dc = cs.x * cos_e - cs.y * sin_e;
    let ds = cs.y * cos_e + cs.x * sin_e;
    let r = f32(radius);
    // x = cx + r cos, y = cy - r sin.
    let x = scaled(radius, whole_cs.x);
    let y = scaled(radius, whole_cs.y);
    let whole = params.centre_whole.xy + vec2(x.whole, -y.whole);
    let part = vec2(
        x.part + (params.centre_part.x - 0.5) + r * (entry.z + dc),
        (params.centre_part.y - 0.5) - y.part - r * (entry.w + ds),
    );
    return RingPoint(point(whole, part), vec2(cs.x + dc, -(cs.y + ds)));
}

// Where pixel (x, y) lies on the rings: the space `k` it is past, the
// sample `s` nearest it on ring `k`, how far between rings `k` and
// `k + 1` (`t`), and its angle past sample `s`.
struct OnRing {
    k: u32,
    s: u32,
    t: f32,
    turn: f32,
}

fn on_ring(x: u32, y: u32, start: u32, settle: bool) -> OnRing {
    let c = vec2<f32>(vec2(i32(x), i32(y)) - params.centre_whole.xy) + 0.5 - params.centre_part.xy;
    var k = start;
    var info = ring(k);
    var s = wrapped(i32(round(atan2(-c.y, c.x) * f32(info.round) / TAU)), info.round);
    var t = 0.0;
    var turn = 0.0;
    for (var round_ = 0; round_ < 8; round_++) {
        let radius = params.inner + k;
        let p = ring_point(radius, info.round, s);
        let o = offset_from(from_point(x, y, p.at), p.dir, f32(radius));
        t = o.out;
        turn = o.turn;
        let ds = i32(round(turn * f32(info.round) / TAU));
        var k_next = k;
        if settle && (t < 0.0 || t >= 1.0) {
            k_next = u32(clamp(i32(k) + i32(floor(t)), 0, i32(params.lines) - 1));
        }
        if (k_next == k && ds == 0) || round_ == 7 {
            break;
        }
        if k_next != k {
            // The same angle on the new ring.
            let next = ring(k_next);
            let a = (f32(s) + turn * f32(info.round) / TAU) / f32(info.round);
            s = wrapped(i32(round(a * f32(next.round))), next.round);
            k = k_next;
            info = next;
            continue;
        }
        s = wrapped(i32(s) + ds, info.round);
    }
    return OnRing(k, s, clamp(t, 0.0, 1.0), turn);
}

// ---- the lines of a batch ----

// A line of the batch: where its points start, how many samples it has,
// and how many samples a turn of it holds (0 for a ray, which does not
// repeat).
struct Line {
    points: u32,
    samples: u32,
    period: u32,
}

fn line_of(l: u32) -> Line {
    if params.spin != 0u {
        let r = ring(params.batch_first + l);
        return Line(r.points, r.samples, r.round);
    }
    return Line(l * (params.samples + 1u), params.samples, 0u);
}

// Where line `l`'s chunks' sums start.
fn chunks_of(l: u32) -> u32 {
    if params.spin != 0u {
        return params.chunk_base + 4u * ring(params.batch_first + l).chunks;
    }
    return params.chunk_base + 4u * l * params.chunks;
}

// Sample `k` of line `l` of the batch.
fn read_sample(l: u32, k: u32) -> vec4<f32> {
    let line = params.batch_first + l;
    if params.spin != 0u {
        let r = ring(line);
        let s = wrapped(r.first + i32(k), r.round);
        return sample_at(ring_point(params.inner + line, r.round, s).at);
    }
    return sample_at(ray_point(line, k));
}

// Reads the samples of one chunk of one line, and sums them.
@compute @workgroup_size(64)
fn read_lines(@builtin(global_invocation_id) gid: vec3<u32>) {
    let l = gid.x;
    let c = gid.y;
    if l >= params.batch_lines {
        return;
    }
    let line = line_of(l);
    let start = c * params.chunk;
    if start >= line.samples {
        return;
    }
    let end = min(start + params.chunk, line.samples);
    var sum = wide_zero();
    var moment = wide_zero();
    for (var k = start; k < end; k++) {
        let x = read_sample(l, k);
        written[(line.points + k) * params.stride] = bitcast<vec4<u32>>(x);
        let q = wide_of(fixed(x));
        sum = wide_add(sum, q);
        if params.linear != 0u {
            moment = wide_add(moment, wide_times(q, i32(k)));
        }
    }
    let at = chunks_of(l) + 4u * c;
    written[at] = sum.lo;
    written[at + 1u] = sum.hi;
    written[at + 2u] = moment.lo;
    written[at + 3u] = moment.hi;
}

// Each chunk's sums become the sums of the chunks before it on its line.
@compute @workgroup_size(64)
fn sums(@builtin(global_invocation_id) gid: vec3<u32>) {
    let l = gid.x;
    if l >= params.batch_lines {
        return;
    }
    let line = line_of(l);
    let base = chunks_of(l);
    let chunks = (line.samples + params.chunk - 1u) / params.chunk;
    var sum = wide_zero();
    var moment = wide_zero();
    for (var c = 0u; c < chunks; c++) {
        let at = base + 4u * c;
        let s = Wide(written[at], written[at + 1u]);
        let m = Wide(written[at + 2u], written[at + 3u]);
        written[at] = sum.lo;
        written[at + 1u] = sum.hi;
        written[at + 2u] = moment.lo;
        written[at + 3u] = moment.hi;
        sum = wide_add(sum, s);
        moment = wide_add(moment, m);
    }
}

// The running sums before every sample of one chunk of one line, and
// after its last sample at the end of the line.
@compute @workgroup_size(64)
fn prefix(@builtin(global_invocation_id) gid: vec3<u32>) {
    let l = gid.x;
    let c = gid.y;
    if l >= params.batch_lines {
        return;
    }
    let line = line_of(l);
    let start = c * params.chunk;
    if start >= line.samples {
        return;
    }
    let end = min(start + params.chunk, line.samples);
    let at = chunks_of(l) + 4u * c;
    var sum = Wide(written[at], written[at + 1u]);
    var moment = Wide(written[at + 2u], written[at + 3u]);
    for (var k = start; k < end; k++) {
        let p = (line.points + k) * params.stride;
        written[p + 1u] = sum.lo;
        written[p + 2u] = sum.hi;
        if params.linear != 0u {
            written[p + 3u] = moment.lo;
            written[p + 4u] = moment.hi;
        }
        let q = wide_of(fixed(bitcast<vec4<f32>>(written[p])));
        sum = wide_add(sum, q);
        if params.linear != 0u {
            moment = wide_add(moment, wide_times(q, i32(k)));
        }
    }
    if end == line.samples {
        let p = (line.points + end) * params.stride;
        written[p + 1u] = sum.lo;
        written[p + 2u] = sum.hi;
        if params.linear != 0u {
            written[p + 3u] = moment.lo;
            written[p + 4u] = moment.hi;
        }
    }
}

// ---- the mean of a pixel along a line ----

fn line_sample(line: Line, k: i32) -> vec4<f32> {
    if k < 0 || k >= i32(line.samples) {
        return vec4(0.0);
    }
    return bitcast<vec4<f32>>(lines[(line.points + u32(k)) * params.stride]);
}

fn line_sum(line: Line, k: u32) -> Wide {
    let p = (line.points + k) * params.stride;
    return Wide(lines[p + 1u], lines[p + 2u]);
}

fn line_moment(line: Line, k: u32) -> Wide {
    let p = (line.points + k) * params.stride;
    return Wide(lines[p + 3u], lines[p + 4u]);
}

// A place on a line: its cell `c`, unwrapped round a ring, and how far
// into it, 0 to 1. Cell `c` holds sample `c` (or `c` round the ring) and
// runs from half a sample before it to half after it.
struct Place {
    c: i32,
    f: f32,
}

// The place `off` samples past sample `base`'s middle; a ray's cells past
// either end are all alike, so they are held to one past each end.
fn place(line: Line, base: i32, off: f32) -> Place {
    let o = clamp(off + 0.5, -1e9, 1e9);
    let w = floor(o);
    var c = base + i32(w);
    if line.period == 0u {
        c = clamp(c, -1, i32(line.samples));
    }
    return Place(c, o - w);
}

// Which turn of a ring a cell is in, and which cell of the turn.
struct Turned {
    q: i32,
    w: i32,
}

fn turned(line: Line, c: i32) -> Turned {
    if line.period == 0u {
        return Turned(0, c);
    }
    let n = i32(line.period);
    var q = c / n;
    if c % n != 0 && c < 0 {
        q -= 1;
    }
    return Turned(q, c - q * n);
}

// What a stretch from `lo` to `hi`, `length` long, adds up to: the
// layer's sum over it and, for a linear fade, its integral times the
// distance from the far end, which is `lo` when `far_lo` holds and `hi`
// otherwise, `to_far` past the middle of cell `c0`. The whole cells
// between come from the exact running sums, their moment about `c0`
// moved to the far end; the parts of the two cells the stretch ends in
// are weighed from the far end directly, so a short stretch adds no large
// numbers that cancel.
struct Sums {
    sum: vec4<f32>,
    moment: vec4<f32>,
}

fn cell_sample(line: Line, c: i32) -> vec4<f32> {
    return line_sample(line, turned(line, c).w);
}

fn sums_between(line: Line, lo: Place, hi: Place, length: f32, far_lo: bool, c0: i32, to_far: f32) -> Sums {
    if lo.c == hi.c {
        let sum = cell_sample(line, lo.c) * length;
        return Sums(sum, sum * select(-length, length, far_lo) / 2.0);
    }
    let a = turned(line, lo.c + 1);
    let b = turned(line, hi.c);
    let n = i32(line.samples);
    var whole = wide_zero();
    var moment = vec4(0.0);
    let turns = min(b.q - a.q, 64);
    for (var t = 0; t <= turns; t++) {
        let q = a.q + t;
        var from_ = 0;
        if t == 0 {
            from_ = a.w;
        }
        var to = i32(line.period);
        if q == b.q || line.period == 0u {
            to = b.w;
        }
        let x0 = u32(clamp(from_, 0, n));
        let x1 = u32(clamp(to, 0, n));
        if x1 <= x0 {
            continue;
        }
        let ds = wide_sub(line_sum(line, x1), line_sum(line, x0));
        whole = wide_add(whole, ds);
        if params.linear != 0u {
            let dm = wide_sub(line_moment(line, x1), line_moment(line, x0));
            moment += wide_float(wide_add(dm, wide_times(ds, q * i32(line.period) - c0)));
        }
    }
    let between = wide_float(whole);
    // The two cells' parts, and how far their middles are from the far end.
    let low = cell_sample(line, lo.c) * (1.0 - lo.f);
    let high = cell_sample(line, hi.c) * hi.f;
    var low_at = (1.0 - lo.f) / 2.0;
    var high_at = length - hi.f / 2.0;
    if !far_lo {
        low_at = low_at - length;
        high_at = -hi.f / 2.0;
    }
    let sum = low + between + high;
    return Sums(sum, (moment - to_far * between) + low * low_at + high * high_at);
}

// `Line::at`: the layer at a place, between the two samples either side.
fn line_at(line: Line, p: Place) -> vec4<f32> {
    var w = p.c;
    if line.period != 0u {
        w = turned(line, p.c).w;
    }
    let up = p.f >= 0.5;
    let k = select(w - 1, w, up);
    let t = select(p.f + 0.5, p.f - 0.5, up);
    let a = line_sample(line, k);
    let b = line_sample(line, k + 1);
    return a + (b - a) * t;
}

// What one far end adds to a mean: the weighted sum and the weight.
struct Part {
    sum: vec4<f32>,
    weight: f32,
}

// The stretch of `line` from `near` (`base` and `near_off`) to the far
// end `far_off` past `base`, `span` = near - far: `Line::stretch`.
fn stretch(line: Line, base: i32, near_off: f32, far_off: f32, span: f32) -> Part {
    if abs(span) < 1e-6 {
        return Part(vec4(0.0), 0.0);
    }
    let near = place(line, base, near_off);
    let far = place(line, base, far_off);
    var c0 = far.c;
    if line.period == 0u {
        c0 = clamp(c0, 0, i32(line.samples));
    }
    let to_far = f32(base - c0) + far_off;
    var s: Sums;
    if span > 0.0 {
        s = sums_between(line, far, near, span, true, c0, to_far);
    } else {
        s = sums_between(line, near, far, -span, false, c0, to_far);
    }
    if params.linear == 0u {
        return Part(s.sum, abs(span));
    }
    // Weighed from 1 at the pixel to 0 at the far end: the moment about
    // the far end over the span.
    return Part(s.moment / span, abs(span) / 2.0);
}

// `Line::mean`: the mean over the stretches to each far end, or the layer
// at the pixel when they have no length.
fn mean(line: Line, base: i32, near: f32, ends: vec4<f32>) -> vec4<f32> {
    let a = stretch(line, base, near, ends.x, ends.y);
    var sum = a.sum;
    var weight = a.weight;
    if params.across != 0u {
        let b = stretch(line, base, near, ends.z, ends.w);
        sum += b.sum;
        weight += b.weight;
    }
    if weight < 1e-2 {
        return line_at(line, place(line, base, near));
    }
    return sum / weight;
}

// The far ends of a stretch `extent` long from `near`, and their spans.
fn ends(near: f32, extent: f32) -> vec4<f32> {
    if params.across != 0u {
        return vec4(near - extent / 2.0, extent / 2.0, near + extent / 2.0, -extent / 2.0);
    }
    return vec4(near - extent, extent, 0.0, 0.0);
}

fn in_batch(i: u32) -> bool {
    let key = keys[i >> 2u][i & 3u];
    return key >= params.batch_first && key < params.batch_first + params.batch_lines - 1u;
}

fn index(gid: vec3<u32>) -> u32 {
    return gid.y * params.row + gid.x;
}

// ---- a zoom ----

// The space between rays pixel `i` lies in.
fn zoom_key(i: u32) -> u32 {
    if i >= params.count {
        return 0u;
    }
    let x = i % params.width;
    let y = i / params.width;
    let from_ = params.view.x;
    let c = vec2<f32>(vec2(i32(x), i32(y)) - params.centre_whole.xy) + 0.5 - params.centre_part.xy;
    let a = atan2(-c.y, c.x) - from_;
    let along = (a - TAU * floor(a / TAU)) / params.view.y;
    let j = u32(clamp(i32(floor(along)), 0, i32(params.lines) - 1));
    return on_ray(x, y, j, true).j;
}

// The keys of four pixels, written whole: a store of part of a vec4 may
// write all of it.
@compute @workgroup_size(256)
fn zoom_keys(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if 4u * i >= params.count {
        return;
    }
    written[i] = vec4(zoom_key(4u * i), zoom_key(4u * i + 1u), zoom_key(4u * i + 2u), zoom_key(4u * i + 3u));
}

@compute @workgroup_size(256)
fn zoom_pixels(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count || !in_batch(i) {
        return;
    }
    let x = i % params.width;
    let y = i / params.width;
    let at = on_ray(x, y, keys[i >> 2u][i & 3u], false);
    let first = params.view.z;
    let radius = first + f32(at.k);
    // The pixel's distance from the centre, as an offset from sample k.
    let near = at.out;
    let r = radius + near;
    var e = ends(near, r * params.view.w);
    // A stretch ends at the centre: it does not cross it.
    if e.x < -radius {
        e.x = -radius;
        e.y = r;
    }
    if e.z < -radius && params.across != 0u {
        e.z = -radius;
        e.w = r;
    }
    let l = at.j - params.batch_first;
    let base = i32(at.k);
    let a = mean(line_of(l), base, near, e);
    let b = mean(line_of(l + 1u), base, near, e);
    written[i] = bitcast<vec4<u32>>(a + (b - a) * at.t);
}

// ---- a spin ----

// The space between rings pixel `i` lies in.
fn spin_key(i: u32) -> u32 {
    if i >= params.count {
        return 0u;
    }
    let x = i % params.width;
    let y = i / params.width;
    let c = vec2<f32>(vec2(i32(x), i32(y)) - params.centre_whole.xy) + 0.5 - params.centre_part.xy;
    let k = u32(clamp(i32(floor(length(c) - f32(params.inner))), 0, i32(params.lines) - 1));
    return on_ring(x, y, k, true).k;
}

@compute @workgroup_size(256)
fn spin_keys(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if 4u * i >= params.count {
        return;
    }
    written[i] = vec4(spin_key(4u * i), spin_key(4u * i + 1u), spin_key(4u * i + 2u), spin_key(4u * i + 3u));
}

@compute @workgroup_size(256)
fn spin_pixels(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count || !in_batch(i) {
        return;
    }
    let x = i % params.width;
    let y = i / params.width;
    let at = on_ring(x, y, keys[i >> 2u][i & 3u], false);
    let l = at.k - params.batch_first;
    let here = ring(at.k);
    let next = ring(at.k + 1u);
    // The pixel's angle in samples of each ring, past a whole sample.
    let base = i32(at.s) - here.first;
    let near = at.turn * f32(here.round) / TAU;
    // s next / here, as a whole sample and a part.
    let grown = at.s * (next.round - here.round);
    let base_next = i32(at.s + grown / here.round) - next.first;
    let near_next = f32(grown % here.round) / f32(here.round) + at.turn * f32(next.round) / TAU;
    let turn = params.view.w;
    let a = mean(line_of(l), base, near, ends(near, turn * f32(here.round) / TAU));
    let b = mean(line_of(l + 1u), base_next, near_next, ends(near_next, turn * f32(next.round) / TAU));
    written[i] = bitcast<vec4<u32>>(a + (b - a) * at.t);
}
