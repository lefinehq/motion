// Float pixels as the 8-bit straight RGBA a frame is written in, as
// `Pixels::to_rgba8` makes them: alpha kept within 0 to 1, each colour
// divided by it, every channel rounded to the nearest of 255 steps, and a
// pixel whose alpha rounds to 0 all zeros. Four bytes a pixel, red first.

struct Params {
    count: u32,
    row: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<storage, read> src: array<vec4<f32>>;

// `(c.clamp(0.0, 1.0) * 255.0).round()`: halves round up, as Rust rounds
// a value that is not negative.
fn byte(c: f32) -> u32 {
    return u32(floor(clamp(c, 0.0, 1.0) * 255.0 + 0.5));
}

@compute @workgroup_size(256)
fn pack(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.y * params.row + gid.x;
    if i >= params.count {
        return;
    }
    let p = src[i];
    let a = clamp(p.a, 0.0, 1.0);
    let alpha = byte(a);
    if alpha == 0u {
        dst[i] = 0u;
        return;
    }
    dst[i] = byte(p.r / a) | (byte(p.g / a) << 8u) | (byte(p.b / a) << 16u) | (alpha << 24u);
}
