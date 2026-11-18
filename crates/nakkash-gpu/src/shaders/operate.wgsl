// Compositing inside effects (`composite.rs`): how an effect's result
// combines with the layer's own pixels by one of the operations, and the
// effects that do little else. Each kernel ports the CPU function named
// beside it; pixels are premultiplied RGBA floats.
//
// The `MODE_*` constants, the blend modes (`blend.wgsl`) and the
// operations (`operation.wgsl`) are written in front of this source.

struct Params {
    // How many pixels, how many invocations one row of workgroups covers,
    // and the grid's width, which places a pixel for the dissolve.
    count: u32,
    row: u32,
    width: u32,
    // The operation, and its blend mode when it blends.
    op: u32,
    mode: u32,
    // Whether the solid fills a box (`BoxCover`), or everywhere.
    boxed: u32,
    // The glow's intensity; the solid composite's source opacity and the
    // solid's opacity.
    k: f32,
    source: f32,
    // The solid's straight colour.
    colour: vec4<f32>,
    // From grid px to the layer's coordinates: x = a·px + c·py + e,
    // y = b·px + d·py + f, at the px's centre.
    back: vec4<f32>,
    back_shift: vec4<f32>,
    // The box, x0, y0, x1, y1, and how far the layer moves for one px
    // along x and y.
    bounds: vec4<f32>,
    step: vec4<f32>,
    opacity: f32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> src: array<vec4<f32>>;

fn index(gid: vec3<u32>) -> u32 {
    return gid.y * params.row + gid.x;
}

// `Operation::pixels`: the effect's result `src` over the pixels `dst`.
@compute @workgroup_size(256)
fn operate(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    dst[i] = operate_one(params.op, params.mode, params.width, dst[i], src[i], i);
}

// `glow`: the spread light times the intensity, each channel at most 1.
@compute @workgroup_size(256)
fn light(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    dst[i] = min(dst[i] * params.k, vec4(1.0));
}

// `BoxCover::at`: one side's overlap with the px, from its centre `at`.
fn side(lo: f32, hi: f32, at: f32, g: f32) -> f32 {
    let near = (at - lo) / g;
    let far = (hi - at) / g;
    return clamp(min(near, 0.5) + min(far, 0.5), 0.0, 1.0);
}

// `solid_composite`: the layer at its source opacity laid over a solid in
// its box.
@compute @workgroup_size(256)
fn solid_composite(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    var cover = 1.0;
    if params.boxed != 0u {
        let width = max(params.width, 1u);
        let px = f32(i % width);
        let py = f32(i / width);
        // `back_shift` is the layer point of px (0, 0)'s centre, so only
        // the px's offset is worked out here.
        let qx = params.back_shift.x + params.back.x * px + params.back.z * py;
        let qy = params.back_shift.y + params.back.y * px + params.back.w * py;
        let b = params.bounds;
        cover = side(b.x, b.z, qx, params.step.x) * side(b.y, b.w, qy, params.step.y);
    }
    let a = params.opacity * cover;
    let solid = vec4(params.colour.rgb * a, a);
    dst[i] = operate_one(params.op, params.mode, params.width, solid, dst[i] * params.source, i);
}
