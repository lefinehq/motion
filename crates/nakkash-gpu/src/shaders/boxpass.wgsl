// One extended box pass along every line of the grid, rows or columns, as
// `blur.rs`'s `box_pass`: each pixel the mean of the `2 whole + 1` around
// it and, by `part`, the two beyond them. Each invocation slides the
// window along one chunk of one line; neighbouring invocations take
// neighbouring lines.
//
// The window's sum is kept exactly, as an integer, where the CPU keeps it
// in f64: a running sum in f32 keeps what values that left the window
// leave of their rounding, so beyond a blur's reach, where the CPU's sum
// is 0 and the pixel transparent, it would leave noise near 1e-11 of
// either sign, which an effect reading straight colour (dividing by
// alpha) turns into any colour at all. An integer sum is exact whatever
// the compiler reorders, which Metal's fast math may.

struct Params {
    // The line's length, and how many lines there are.
    n: u32,
    lines: u32,
    // The step between pixels along a line, and between lines.
    along: u32,
    across: u32,
    // How many pixels each invocation slides over.
    chunk: u32,
    whole: u32,
    part: f32,
    // The box's width, `2 whole + 1 + 2 part`.
    width: f32,
    // Whether the edge pixels repeat beyond the edges, else transparent.
    repeat: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> src: array<vec4<f32>>;

// A sum kept exactly, one for each channel: a 96-bit two's-complement
// integer in units of 2^-64, as three 32-bit limbs, the lowest first. It
// holds every float from 2^-64 up to 2^15 in size, and the sum of a window
// of 2^16 of them; smaller ones are dropped, larger ones clamped.
struct Exact {
    lo: vec4<u32>,
    mid: vec4<u32>,
    hi: vec4<u32>,
}

// The limb `k` of `m`, a float's 24-bit significand, placed `s` bits up.
fn limb(m: vec4<u32>, s: vec4<i32>, k: i32) -> vec4<u32> {
    let o = s - vec4(32 * k);
    let up = m << vec4<u32>(max(o, vec4(0)));
    let down = m >> vec4<u32>(max(-o, vec4(0)));
    let shifted = select(down, up, o >= vec4(0));
    return select(vec4(0u), shifted, (o > vec4(-24)) & (o < vec4(32)));
}

fn negated(a: Exact) -> Exact {
    let lo = ~a.lo + vec4(1u);
    let c0 = select(vec4(0u), vec4(1u), lo == vec4(0u));
    let mid = ~a.mid + c0;
    let c1 = select(vec4(0u), vec4(1u), (mid == vec4(0u)) & (c0 == vec4(1u)));
    return Exact(lo, mid, ~a.hi + c1);
}

fn added(a: Exact, b: Exact) -> Exact {
    let lo = a.lo + b.lo;
    let c0 = select(vec4(0u), vec4(1u), lo < a.lo);
    let mid0 = a.mid + b.mid;
    let mid = mid0 + c0;
    let c1 = select(vec4(0u), vec4(1u), (mid0 < a.mid) | (mid < mid0));
    return Exact(lo, mid, a.hi + b.hi + c1);
}

// `x` exactly, but for what lies below 2^-64.
fn exact(x: vec4<f32>) -> Exact {
    let bits = bitcast<vec4<u32>>(clamp(x, vec4(-32768.0), vec4(32768.0)));
    let e = (bits >> vec4(23u)) & vec4(255u);
    let m = (bits & vec4(0x7fffffu)) | select(vec4(0u), vec4(0x800000u), e != vec4(0u));
    // The significand's lowest bit is worth 2^(e - 150), subnormals'
    // 2^-149: in units of 2^-64, 2^(e - 86).
    let s = vec4<i32>(max(e, vec4(1u))) - vec4(86);
    let magnitude = Exact(limb(m, s, 0), limb(m, s, 1), limb(m, s, 2));
    let negative = (bits >> vec4(31u)) == vec4(1u);
    let flipped = negated(magnitude);
    return Exact(
        select(magnitude.lo, flipped.lo, negative),
        select(magnitude.mid, flipped.mid, negative),
        select(magnitude.hi, flipped.hi, negative),
    );
}

// The sum as a float: each limb's magnitude rounded, added smallest first,
// none cancelling another.
fn float(a: Exact) -> vec4<f32> {
    let negative = (a.hi >> vec4(31u)) == vec4(1u);
    let flipped = negated(a);
    let lo = select(a.lo, flipped.lo, negative);
    let mid = select(a.mid, flipped.mid, negative);
    let hi = select(a.hi, flipped.hi, negative);
    let magnitude = vec4<f32>(lo) * 5.421010862427522e-20 + vec4<f32>(mid) * 2.3283064365386963e-10
        + vec4<f32>(hi);
    return select(magnitude, -magnitude, negative);
}

fn at(base: u32, k: i32) -> vec4<f32> {
    var j = k;
    if j < 0 {
        if params.repeat == 0u {
            return vec4(0.0);
        }
        j = 0;
    }
    if j >= i32(params.n) {
        if params.repeat == 0u {
            return vec4(0.0);
        }
        j = i32(params.n) - 1;
    }
    return src[base + u32(j) * params.along];
}

@compute @workgroup_size(64)
fn box_pass(@builtin(global_invocation_id) gid: vec3<u32>) {
    let line = gid.x;
    let start = gid.y * params.chunk;
    if line >= params.lines || start >= params.n {
        return;
    }
    let end = min(start + params.chunk, params.n);
    let base = line * params.across;
    let w = i32(params.whole);
    var sum = Exact(vec4(0u), vec4(0u), vec4(0u));
    for (var j = -w; j <= w; j++) {
        sum = added(sum, exact(at(base, i32(start) + j)));
    }
    for (var i = start; i < end; i++) {
        let k = i32(i);
        let low = at(base, k - w - 1);
        let high = at(base, k + w + 1);
        dst[base + i * params.along] = (float(sum) + params.part * (low + high)) / params.width;
        sum = added(sum, exact(high));
        sum = added(sum, negated(exact(at(base, k - w))));
    }
}
