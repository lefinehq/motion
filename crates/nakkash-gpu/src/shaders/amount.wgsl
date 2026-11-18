// How much of a source is laid at each pixel: a constant times each share
// in turn, as `Amount` multiplies them on the CPU. Each kernel multiplies
// one share into `amount`, starting from the constant when `first` is set.

struct Params {
    count: u32,
    row: u32,
    first: u32,
    // A matte's mode: 0 alpha, 1 inverted alpha, 2 luma, 3 inverted luma.
    mode: u32,
    opacity: f32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> amount: array<f32>;
@group(0) @binding(2) var<storage, read> values: array<f32>;
@group(0) @binding(3) var<storage, read> pixels: array<vec4<f32>>;

fn index(gid: vec3<u32>) -> u32 {
    return gid.y * params.row + gid.x;
}

fn before(i: u32) -> f32 {
    if params.first != 0u {
        return params.opacity;
    }
    return amount[i];
}

// A share worked out on the CPU, one value a pixel.
@compute @workgroup_size(256)
fn share_values(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    amount[i] = before(i) * values[i];
}

// The alpha of pixels, unclamped, as a filter layer's region reads it.
@compute @workgroup_size(256)
fn share_alpha(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    amount[i] = before(i) * pixels[i].a;
}

// A matte read from its source's pixels, as the painter's `matte_read`.
@compute @workgroup_size(256)
fn share_matte(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let p = pixels[i];
    let a = clamp(p.a, 0.0, 1.0);
    let luma = clamp(p.r * 0.2126 + p.g * 0.7152 + p.b * 0.0722, 0.0, 1.0);
    var v = a;
    switch params.mode {
        case 1u: { v = 1.0 - a; }
        case 2u: { v = luma; }
        case 3u: { v = 1.0 - luma; }
        default: {}
    }
    amount[i] = before(i) * v;
}
