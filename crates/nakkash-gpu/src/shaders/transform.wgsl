// `Pixels::transformed`: the pixel at a point is the one the transform
// brings there, sampled bilinearly from the four nearest, transparent
// beyond the edges.

struct Params {
    width: u32,
    height: u32,
    count: u32,
    row: u32,
    // The inverse transform, as an affine map's six coefficients.
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> src: array<vec4<f32>>;

fn pixel_at(x: i32, y: i32) -> vec4<f32> {
    if x < 0 || y < 0 || x >= i32(params.width) || y >= i32(params.height) {
        return vec4(0.0);
    }
    return src[u32(y) * params.width + u32(x)];
}

@compute @workgroup_size(256)
fn transformed(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.y * params.row + gid.x;
    if i >= params.count {
        return;
    }
    let px = f32(i % params.width) + 0.5;
    let py = f32(i / params.width) + 0.5;
    let x = params.a * px + params.c * py + params.e - 0.5;
    let y = params.b * px + params.d * py + params.f - 0.5;
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let column0 = i32(x0);
    let row0 = i32(y0);
    let top = mix(pixel_at(column0, row0), pixel_at(column0 + 1, row0), fx);
    let bottom = mix(pixel_at(column0, row0 + 1), pixel_at(column0 + 1, row0 + 1), fx);
    dst[i] = mix(top, bottom, fy);
}
