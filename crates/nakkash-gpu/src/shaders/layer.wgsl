// Drawing in a layer's own coordinates, as the generators of `generate.rs`
// do (`each_in_layer`): each pixel the layer covers is worked out at its
// centre's point in the layer, from its straight colour, and takes the
// result as a share of the layer's alpha. Written in front of every
// family of generators, after the operations (`operation.wgsl`).
//
// The layer's points come from lines worked out on the CPU in 64 bits:
// the x and y of the layer at each column's centre (row 0's top edge
// left out), then those each row adds. For a layer the transform keeps
// upright, a row adds nothing to x and a column nothing to y, so each
// point is the one the CPU rounds to 32 bits, to the bit.

@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> lines: array<f32>;

// The layer's point at the centre of pixel (`px`, `py`) of a grid `w` by
// `h` pixels: the columns' x, the columns' y, the rows' x, the rows' y,
// each a line, from the start of `lines`.
fn layer_point(px: u32, py: u32, w: u32, h: u32) -> vec2<f32> {
    return vec2(lines[px] + lines[2u * w + py], lines[w + px] + lines[2u * w + h + py]);
}

// `a / b`, rounded to the nearest float as the CPU's division is: the
// GPU's own division may be a bit or two off. A first quotient of the
// sizes is corrected by its remainder, then the nearest of it and its two
// neighbours is kept by their exact remainders (`fma`), ties to the even
// one.
fn divided(a: f32, b: f32) -> f32 {
    let m = abs(a);
    let n = abs(b);
    if m == 0.0 {
        return a / b;
    }
    var q = m / n;
    q += fma(-q, n, m) / n;
    let bits = bitcast<u32>(q);
    var best = q;
    var least = abs(fma(-q, n, m));
    for (var k = 0u; k < 2u; k++) {
        let c = bitcast<f32>(select(bits + 1u, bits - 1u, k == 1u));
        let r = abs(fma(-c, n, m));
        if r < least || (r == least && (bitcast<u32>(c) & 1u) == 0u) {
            best = c;
            least = r;
        }
    }
    if (a < 0.0) != (b < 0.0) {
        return -best;
    }
    return best;
}

// `each_in_layer`: the pixel a covered pixel of coverage `coverage` takes
// from the straight colour and share `paint` gives for it.
fn painted(paint: vec4<f32>, coverage: f32) -> vec4<f32> {
    let c = clamp(paint, vec4(0.0), vec4(1.0));
    let a = c.a * coverage;
    return vec4(c.rgb * a, a);
}

// `each_in_layer`: the straight colour of a pixel of coverage `coverage`,
// opaque.
fn straight_of(pixel: vec4<f32>, coverage: f32) -> vec4<f32> {
    return vec4(pixel.rgb / coverage, 1.0);
}

// `composite`: a generator's straight colour `over` at `amount`, combined
// with the layer's straight colour `under` by the operation `op` (and
// blend `mode`), at pixel `i` of a grid `width` wide; straight colour and
// a share of the layer's alpha.
fn composite(op: u32, mode: u32, width: u32, under: vec4<f32>, over: vec3<f32>, amount: f32, i: u32) -> vec4<f32> {
    let p = operate_one(op, mode, width, vec4(under.rgb, 1.0), vec4(over * amount, amount), i);
    if p.a > 0.0 {
        return vec4(p.rgb / p.a, p.a);
    }
    return vec4(0.0);
}
