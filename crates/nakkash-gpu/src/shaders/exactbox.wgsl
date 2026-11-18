// One extended box pass along every line of the grid, rows or columns, as
// `blur.rs`'s `box_pass`, its window summed exactly: as 64-bit
// fixed-point numbers of 2^-40, two 32-bit words a channel, which integer
// arithmetic adds and takes away without rounding. The float box pass
// keeps a sum to a float's precision of the whole window, so a faint pixel
// beside bright ones is off by a share of the bright ones; here every
// pixel keeps a float's precision of its own value, as the CPU's 64-bit
// sums keep it. A layer's colour divided by its alpha, faint at the edges
// of what it drew, is where that shows.
//
// Values are held exactly to 2^-40 below 128 in magnitude; the window's
// sum to 2^23.

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
// shifted by its exponent, in integers.
fn fixed(v: vec4<f32>) -> Wide {
    let bits = bitcast<vec4<u32>>(v);
    let e = (bits >> vec4(23u)) & vec4(255u);
    // Below 2^-41 is nothing; subnormals are below it.
    let normal = e >= vec4(86u);
    // At or past 128, held at just below it.
    let big = e > vec4(133u);
    let m = select((bits & vec4(0x7fffffu)) | vec4(0x800000u), vec4(0xffffffu), big);
    let ee = select(e, vec4(133u), big);
    // The significand's place: shifted left by ee - 110, or right by
    // 110 - ee with rounding.
    let left = ee >= vec4(110u);
    let ls = select(vec4(0u), ee - vec4(110u), left);
    let rs = select(vec4(110u) - ee, vec4(0u), left);
    let rounded = (m + select(vec4(0u), vec4(1u) << (rs - vec4(1u)), rs > vec4(0u))) >> rs;
    let low_word = ls < vec4(32u);
    let lo = select(vec4(0u), m << (ls & vec4(31u)), low_word);
    let hi = select(
        m << ((ls - vec4(32u)) & vec4(31u)),
        select(vec4(0u), m >> ((vec4(32u) - ls) & vec4(31u)), ls > vec4(0u)),
        low_word,
    );
    var w = Wide(select(rounded, lo, left), select(vec4(0u), hi, left));
    w = Wide(select(vec4(0u), w.lo, normal), select(vec4(0u), w.hi, normal));
    // Negative values: the two's complement.
    let negative = (bits >> vec4(31u)) == vec4(1u);
    let flipped = wide_sub(Wide(vec4(0u), vec4(0u)), w);
    return Wide(select(w.lo, flipped.lo, negative), select(w.hi, flipped.hi, negative));
}

// A fixed-point number as a float.
fn float_of(a: Wide) -> vec4<f32> {
    let small = (a.hi == vec4(0u) & a.lo < vec4(0x80000000u))
        | (a.hi == vec4(0xffffffffu) & a.lo >= vec4(0x80000000u));
    let exact = vec4<f32>(bitcast<vec4<i32>>(a.lo));
    let big = vec4<f32>(bitcast<vec4<i32>>(a.hi)) * 4294967296.0 + vec4<f32>(a.lo);
    // 2^-40.
    return select(big, exact, small) * 9.094947017729282e-13;
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
fn exact_box(@builtin(global_invocation_id) gid: vec3<u32>) {
    let line = gid.x;
    let start = gid.y * params.chunk;
    if line >= params.lines || start >= params.n {
        return;
    }
    let end = min(start + params.chunk, params.n);
    let base = line * params.across;
    let w = i32(params.whole);
    var sum = Wide(vec4(0u), vec4(0u));
    for (var j = -w; j <= w; j++) {
        sum = wide_add(sum, fixed(at(base, i32(start) + j)));
    }
    for (var i = start; i < end; i++) {
        let k = i32(i);
        let low = at(base, k - w - 1);
        let high = at(base, k + w + 1);
        dst[base + i * params.along] = (float_of(sum) + params.part * (low + high)) / params.width;
        sum = wide_add(sum, wide_sub(fixed(high), fixed(at(base, k - w))));
    }
}
