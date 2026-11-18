// What the rasteriser drew, premultiplied RGBA at 8 bits a channel, as
// floats: each channel through a table of `c / 255` worked out on the CPU,
// so the floats are those `Pixels::from_pixmap` gives, bit for bit.

struct Params {
    count: u32,
    row: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> bytes: array<u32>;
@group(0) @binding(3) var<storage, read> table: array<f32, 256>;

@compute @workgroup_size(256)
fn unpack(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.y * params.row + gid.x;
    if i >= params.count {
        return;
    }
    let p = bytes[i];
    dst[i] = vec4(
        table[p & 255u],
        table[(p >> 8u) & 255u],
        table[(p >> 16u) & 255u],
        table[p >> 24u],
    );
}
