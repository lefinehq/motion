// Fractal noise (`generate.rs`, `FractalNoise`): grey value noise summed
// over octaves, shaped and combined with the layer by an operation. The
// lattice hash and the blends between lattice values are the CPU's, in
// the same 32-bit integer and float steps.
//
// Where an octave's point lies decides which lattice cell it falls in, and
// block noise jumps from cell to cell, so the points are worked out as the
// CPU rounds them, step by step: each product rounded on its own
// (`product`), sums in the CPU's order (`then`), divisions rounded to the
// nearest (`divided`). Left to itself the GPU fuses a product into the
// addition after it and divides by a reciprocal, each a bit apart.
//
// The operations (`operation.wgsl`) and the layer's points (`layer.wgsl`)
// are written in front of this source.

struct Params {
    // How many pixels, how many invocations one row of workgroups covers,
    // and the grid's size.
    count: u32,
    row: u32,
    width: u32,
    height: u32,
    // The operation and its blend mode; how many octaves; what `lines`
    // holds, by the `LINES_*` constants.
    op: u32,
    mode: u32,
    octaves: u32,
    lines: u32,
    // The first octave's seed, and the fractal type, noise type and
    // overflow by their order in `Fractal`, `NoiseKind` and `Overflow`;
    // whether the level is inverted.
    seed: u32,
    fractal: u32,
    kind: u32,
    overflow: u32,
    invert: u32,
    contrast: f32,
    brightness: f32,
    complexity: f32,
    influence: f32,
    scaling: f32,
    evolution: f32,
    opacity: f32,
    // The sine and cosine of the rotation, then of the sub-rotation.
    turn: vec4<f32>,
    // A cell's width and height in the layer's px, and the offset.
    scale: vec4<f32>,
    // The sub-offset in cells, then 0 and 1, which the kernel cannot see
    // through and so cannot fold into the steps they keep apart.
    step: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;

// What `lines` holds: the layer's lines (`layer.wgsl`), whose points are
// the CPU's to the bit in a layer its transform keeps upright; or the
// layer's point at every pixel, x then y, for block noise in a layer its
// transform turns.
const LINES_POINTS: u32 = 1u;

const FRACTAL_TURBULENT_SMOOTH: u32 = 1u;
const FRACTAL_TURBULENT_SHARP: u32 = 3u;
const FRACTAL_SMEARY: u32 = 4u;
const FRACTAL_SWIRLY: u32 = 5u;
const KIND_SPLINE: u32 = 0u;
const KIND_SOFT_LINEAR: u32 = 1u;
const KIND_BLOCK: u32 = 3u;
const OVERFLOW_SOFT_CLAMP: u32 = 1u;
const OVERFLOW_WRAP_BACK: u32 = 2u;
// `f32::EPSILON` and `TAU` as the CPU rounds them.
const EPSILON: f32 = 1.1920929e-7;
const TAU: f32 = 6.2831855;

// `lattice`: a lattice point's value, 0 to 1, hashed from its coordinates
// and seed.
fn lattice(at: vec3<i32>, seed: u32) -> f32 {
    var h = (seed * 0x9E3779B9u)
        ^ (bitcast<u32>(at.x) * 0x85EBCA6Bu)
        ^ (bitcast<u32>(at.y) * 0xC2B2AE35u)
        ^ (bitcast<u32>(at.z) * 0x27D4EB2Fu);
    h ^= h >> 15u;
    h *= 0x2C1B3C6Du;
    h ^= h >> 12u;
    h *= 0x297A2D39u;
    h ^= h >> 15u;
    return f32(h >> 8u) / 16777216.0;
}

// How far between lattice values `t` of the way across a cell lies, by
// the noise type.
fn ease(t: f32, kind: u32) -> f32 {
    if kind == KIND_SPLINE {
        return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    }
    if kind == KIND_SOFT_LINEAR {
        return t * t * (3.0 - 2.0 * t);
    }
    return t;
}

// `as i32` of a whole float: saturating at i32's ends, as the CPU's cast
// is (a plain conversion stops short of them).
fn whole(c: f32) -> i32 {
    if c >= 2147483648.0 {
        return 2147483647;
    }
    if c < -2147483648.0 {
        return -2147483648;
    }
    return i32(c);
}

// `sum / total` for `total` above 0, rounded to the nearest: both brought
// down by a power of two first when `total` is so large that the GPU's
// division, which takes the reciprocal, would lose it below the smallest
// normal float.
fn ratio(sum: f32, total: f32) -> f32 {
    if total > 1e30 {
        return divided(ldexp(sum, -64), ldexp(total, -64));
    }
    return divided(sum, total);
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    return a + (b - a) * t;
}

// `value_noise`: value noise at `p`, 0 to 1.
fn value_noise(p: vec3<f32>, seed: u32, kind: u32) -> f32 {
    let cell = floor(p);
    let f = p - cell;
    let at = vec3(whole(cell.x), whole(cell.y), whole(cell.z));
    if kind == KIND_BLOCK {
        return lattice(at, seed);
    }
    let w = vec3(ease(f.x, kind), ease(f.y, kind), ease(f.z, kind));
    var planes: array<f32, 2>;
    for (var dz = 0; dz < 2; dz++) {
        let c00 = lattice(at + vec3(0, 0, dz), seed);
        let c10 = lattice(at + vec3(1, 0, dz), seed);
        let c01 = lattice(at + vec3(0, 1, dz), seed);
        let c11 = lattice(at + vec3(1, 1, dz), seed);
        planes[dz] = lerp(lerp(c00, c10, w.x), lerp(c01, c11, w.x), w.y);
    }
    return lerp(planes[0], planes[1], w.z);
}

// `a * b` rounded on its own, as the CPU rounds a product before adding
// it to anything.
fn product(a: f32, b: f32) -> f32 {
    return fma(a, b, params.step.z);
}

// `a + b`, rounded, before anything is added to it.
fn then(a: f32, b: f32) -> f32 {
    return fma(a, params.step.w, b);
}

// The first octave's point for the layer's point `at`.
fn first(at: vec2<f32>) -> vec2<f32> {
    let d = at - params.scale.zw;
    let sin = params.turn.x;
    let cos = params.turn.y;
    return vec2(
        divided(product(d.x, cos) + product(d.y, sin), params.scale.x),
        divided(product(-d.x, sin) + product(d.y, cos), params.scale.y),
    );
}

// The next octave's point from this one's, scaled down and warped.
fn next(scaled: vec2<f32>) -> vec2<f32> {
    let sin = params.turn.z;
    let cos = params.turn.w;
    return vec2(
        then(product(scaled.x, cos) + product(scaled.y, sin), params.step.x),
        then(product(-scaled.x, sin) + product(scaled.y, cos), params.step.y),
    );
}

// `Warp::by`: how far a warping type moves the next octave at `level`.
fn warp(level: f32) -> vec2<f32> {
    if params.fractal == FRACTAL_SMEARY {
        return vec2(2.0 * level - 1.0, 0.0);
    }
    let turn = TAU * level;
    return vec2(0.5 * cos(turn), 0.5 * sin(turn));
}

// `FractalNoise::at`: the level at pixel `i`, (`px`, `py`).
fn level(i: u32, px: u32, py: u32) -> f32 {
    var at: vec2<f32>;
    if params.lines == LINES_POINTS {
        at = vec2(lines[2u * i], lines[2u * i + 1u]);
    } else {
        at = layer_point(px, py, params.width, params.height);
    }
    var p = first(at);
    let turbulent = params.fractal >= 1u && params.fractal <= 3u;
    let warps = params.fractal == FRACTAL_SMEARY || params.fractal == FRACTAL_SWIRLY;
    var sum = 0.0;
    var total = 0.0;
    var weight = 1.0;
    for (var octave = 0u; octave < params.octaves; octave++) {
        // The last octave weighs by the fraction of complexity left.
        let share = min(params.complexity - f32(octave), 1.0);
        var n = value_noise(vec3(p, params.evolution), params.seed + octave, params.kind);
        if turbulent {
            n = abs(2.0 * n - 1.0);
        }
        sum += product(product(n, weight), share);
        total += product(weight, share);
        weight *= params.influence;
        var scaled = vec2(divided(p.x, params.scaling), divided(p.y, params.scaling));
        // A warping type moves the next octave by the level so far.
        if warps {
            scaled += warp(ratio(sum, max(total, EPSILON)));
        }
        p = next(scaled);
    }
    var v = 0.5;
    if total > 0.0 {
        v = ratio(sum, total);
    }
    if params.fractal == FRACTAL_TURBULENT_SMOOTH {
        v = v * v * (3.0 - 2.0 * v);
    } else if params.fractal == FRACTAL_TURBULENT_SHARP {
        v = sqrt(v);
    }
    if params.invert != 0u {
        v = 1.0 - v;
    }
    v = (v - 0.5) * params.contrast + 0.5 + params.brightness;
    if params.overflow == OVERFLOW_SOFT_CLAMP {
        return 1.0 / (1.0 + exp(-4.0 * (v - 0.5)));
    }
    if params.overflow == OVERFLOW_WRAP_BACK {
        // `rem_euclid(2.0)`: the remainder, which is exact, made positive.
        var t = v % 2.0;
        if t < 0.0 {
            t += 2.0;
        }
        if t > 1.0 {
            return 2.0 - t;
        }
        return t;
    }
    return clamp(v, 0.0, 1.0);
}

// `fractal_noise`: the noise over the layer by its operation.
@compute @workgroup_size(256)
fn fractal_noise(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.y * params.row + gid.x;
    if i >= params.count {
        return;
    }
    let pixel = dst[i];
    let coverage = pixel.a;
    if coverage <= 0.0 {
        return;
    }
    let w = max(params.width, 1u);
    let v = level(i, i % w, i / w);
    let under = straight_of(pixel, coverage);
    dst[i] = painted(composite(params.op, params.mode, w, under, vec3(v), params.opacity, i), coverage);
}
