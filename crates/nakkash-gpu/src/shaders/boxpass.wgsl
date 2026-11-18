// One extended box pass along every line of the grid, rows or columns, as
// `blur.rs`'s `box_pass`: each pixel the mean of the `2 whole + 1` around
// it and, by `part`, the two beyond them. Each invocation slides the
// window along one chunk of one line; neighbouring invocations take
// neighbouring lines.

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
    var sum = vec4(0.0);
    for (var j = -w; j <= w; j++) {
        sum += at(base, i32(start) + j);
    }
    for (var i = start; i < end; i++) {
        let k = i32(i);
        let low = at(base, k - w - 1);
        let high = at(base, k + w + 1);
        dst[base + i * params.along] = (sum + params.part * (low + high)) / params.width;
        sum += high - at(base, k - w);
    }
}
