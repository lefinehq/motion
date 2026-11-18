// A projected layer's buffer laid onto a grid (`projected.rs`, `lay_tile`
// and `Footprint::sample`): each px of the grid seen back through each
// landing onto the buffer, its footprint there from the map's Jacobian,
// sampled trilinearly from the buffer's halved copies (`halve.wgsl`) with
// up to `MAX_ANISOTROPY` probes along its long axis, and a share of it
// added. The CPU works in 64-bit floats, this in 32: each landing's map is
// given from its first px, worked out on the CPU, so only the steps from
// there are summed here.

const MAX_ANISOTROPY: f32 = 16.0;
const MIN_POSITIVE: f32 = 1.17549435e-38;

struct Params {
    count: u32,
    row: u32,
    // The grid's width; the box of px laid, `x0`, `y0` and its width.
    width: u32,
    x0: u32,
    y0: u32,
    box_width: u32,
    // How many landings and halved copies; whether the px are 8-bit.
    landings: u32,
    halves: u32,
    bytes: u32,
    share: f32,
    // The buffer's size in px.
    buffer_width: f32,
    buffer_height: f32,
}

// One landing: the map back at its first px's centre, how it changes one
// px right and one px down, as `(x, y, w)`, and the px it lands on.
struct Landing {
    origin: vec4<f32>,
    right: vec4<f32>,
    down: vec4<f32>,
    landed: vec4<u32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> chain: array<u32>;
// Each copy's first px, width and height, the buffer itself first.
@group(0) @binding(3) var<storage, read> levels: array<vec4<u32>>;
@group(0) @binding(4) var<storage, read> landings: array<Landing>;

// The px at `x`, `y` of copy `level`, in steps of a channel; clear beyond
// its edges.
fn texel(level: u32, x: i32, y: i32) -> vec4<f32> {
    let l = levels[level];
    if x < 0 || y < 0 || u32(x) >= l.y || u32(y) >= l.z {
        return vec4(0.0);
    }
    let i = l.x + u32(y) * l.y + u32(x);
    if params.bytes != 0u {
        let p = chain[i];
        return vec4(f32(p & 0xffu), f32((p >> 8u) & 0xffu), f32((p >> 16u) & 0xffu), f32(p >> 24u));
    }
    let j = 4u * i;
    return vec4(bitcast<f32>(chain[j]), bitcast<f32>(chain[j + 1u]), bitcast<f32>(chain[j + 2u]), bitcast<f32>(chain[j + 3u]));
}

// `Texture::bilinear`: the colour at `p` of copy `level`, px centres at
// half steps, from the four nearest px, 0 to 1.
fn bilinear(level: u32, p: vec2<f32>) -> vec4<f32> {
    let q = p - vec2(0.5);
    let f0 = floor(q);
    let f = q - f0;
    let x0 = i32(f0.x);
    let y0 = i32(f0.y);
    var k = 1.0;
    if params.bytes != 0u {
        k = 1.0 / 255.0;
    }
    let w0 = (1.0 - f.x) * (1.0 - f.y) * k;
    let w1 = f.x * (1.0 - f.y) * k;
    let w2 = (1.0 - f.x) * f.y * k;
    let w3 = f.x * f.y * k;
    return texel(level, x0, y0) * w0 + texel(level, x0 + 1, y0) * w1 + texel(level, x0, y0 + 1) * w2 + texel(level, x0 + 1, y0 + 1) * w3;
}

// `Chain::trilinear`: the colour at buffer point `b` over a footprint
// `2^octave` buffer px wide, between the two copies nearest that size.
fn trilinear(b: vec2<f32>, octave_in: f32) -> vec4<f32> {
    let octave = max(octave_in, 0.0);
    let below = floor(octave);
    let t = octave - below;
    let last = params.halves;
    if below >= f32(last) {
        return bilinear(last, b * exp2(-f32(last)));
    }
    let n = u32(below);
    let lo = bilinear(n, b * exp2(-f32(n)));
    if t <= 0.0 {
        return lo;
    }
    let hi = bilinear(n + 1u, b * exp2(-f32(n + 1u)));
    return lo + (hi - lo) * t;
}

// `log2`, as the CPU approximates it: the exponent, and the mantissa's
// from the same cubic.
fn log2_fit(x: f32) -> f32 {
    let bits = bitcast<u32>(x);
    let exponent = f32((bits >> 23u) & 0xffu) - 127.0;
    let m = bitcast<f32>((bits & 0x7fffffu) | 0x3f800000u) - 1.0;
    let a = 1.4229;
    let b = -0.5821;
    return exponent + m * (a + m * (b + m * (1.0 - a - b)));
}

// `long_axis`: the unit direction along which the footprint is longest.
fn long_axis(j: vec4<f32>, long2: f32) -> vec2<f32> {
    let p = j.x * j.x + j.y * j.y;
    let q = j.x * j.z + j.y * j.w;
    let r = j.z * j.z + j.w * j.w;
    let one = vec2(q, long2 - p);
    let other = vec2(long2 - r, q);
    var v = other;
    if dot(one, one) >= dot(other, other) {
        v = one;
    }
    let length_v = length(v);
    if length_v > 0.0 {
        return v / length_v;
    }
    if p >= r {
        return vec2(1.0, 0.0);
    }
    return vec2(0.0, 1.0);
}

// `Footprint::sample`: the colour over the footprint of Jacobian `j`
// (rows `[a, b]`, `[c, d]`) at buffer point `b`.
fn sample(j: vec4<f32>, long2: f32, short2: f32, b: vec2<f32>) -> vec4<f32> {
    if long2 <= 1.0 {
        return bilinear(0u, b);
    }
    var probes = 1.0;
    if !(long2 < 2.25 * short2) {
        probes = min(floor(sqrt(long2 / max(short2, MIN_POSITIVE)) + 0.5), MAX_ANISOTROPY);
    }
    let octave = log2_fit(max(long2 / (probes * probes), short2)) / 2.0;
    if probes == 1.0 {
        return trilinear(b, octave);
    }
    let axis = long_axis(j, long2);
    let step = sqrt(long2) / probes;
    var sum = vec4(0.0);
    let count = u32(probes);
    for (var k = 0u; k < count; k++) {
        let t = (f32(k) + 0.5 - probes / 2.0) * step;
        sum += trilinear(b + axis * t, octave);
    }
    return sum / probes;
}

@compute @workgroup_size(256)
fn project(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.y * params.row + gid.x;
    if i >= params.count {
        return;
    }
    let x = params.x0 + i % params.box_width;
    let y = params.y0 + i / params.box_width;
    var total = vec4(0.0);
    var any = false;
    for (var n = 0u; n < params.landings; n++) {
        let landing = landings[n];
        let r = landing.landed;
        if x < r.x || x >= r.z || y < r.y || y >= r.w {
            continue;
        }
        let dx = f32(x - r.x);
        let dy = f32(y - r.y);
        let q = landing.origin.xyz + landing.right.xyz * dx + landing.down.xyz * dy;
        let w = q.z;
        // The ray meets the plane behind the camera, or at infinity.
        if w <= MIN_POSITIVE {
            continue;
        }
        let inverse = 1.0 / w;
        let b = q.xy * inverse;
        // The Jacobian of the map back: its first two columns.
        let g0 = vec2(landing.right.x, landing.down.x);
        let g1 = vec2(landing.right.y, landing.down.y);
        let g2 = vec2(landing.right.z, landing.down.z);
        let j = vec4((g0 - b.x * g2) * inverse, (g1 - b.y * g2) * inverse);
        // `Footprint::new`.
        let fp = j.x * j.x + j.y * j.y;
        let fq = j.x * j.z + j.y * j.w;
        let fr = j.z * j.z + j.w * j.w;
        let half = (fp + fr) / 2.0;
        let root = sqrt(((fp - fr) / 2.0) * ((fp - fr) / 2.0) + fq * fq);
        let long2 = half + root;
        let short2 = max(half - root, 0.0);
        let bw = params.buffer_width;
        let bh = params.buffer_height;
        let inside = b.x >= 0.0 && b.y >= 0.0 && b.x <= bw && b.y <= bh;
        if !inside {
            let reach = sqrt(long2) + 1.0;
            if !(b.x > -reach && b.y > -reach && b.x < bw + reach && b.y < bh + reach) {
                continue;
            }
        }
        total += sample(j, long2, short2, b) * params.share;
        any = true;
    }
    if any {
        dst[y * params.width + x] = total;
    }
}
