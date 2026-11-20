// Per-pixel kernels of the float canvas: laying by a blend mode, mixing,
// cutting, scaling, and the per-pixel steps of the glow and the shadow.
// Each ports one CPU function of `nakkash-raster`, named beside it; pixels
// are premultiplied RGBA floats, as `Pixels` holds them.
//
// The blend modes' `MODE_*` constants and their functions (`blend.wgsl`)
// are written in front of this source, the constants from the
// rasteriser's own table, so a mode's number is its row there.

struct Params {
    // How many pixels, and how many invocations one row of workgroups
    // covers: a dispatch is two-dimensional past 65535 workgroups.
    count: u32,
    row: u32,
    // A blend mode's row, for `lay`.
    mode: u32,
    // Whether `amount` holds one value a pixel; else `k` is the amount.
    per_pixel: u32,
    k: f32,
    // The glow's threshold and intensity, the shadow's opacity.
    threshold: f32,
    intensity: f32,
    opacity: f32,
    // The glow's luma weights, or the shadow's colour.
    colour: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> amount: array<f32>;

fn index(gid: vec3<u32>) -> u32 {
    return gid.y * params.row + gid.x;
}

// The amount at pixel `i`, kept within 0 to 1.
fn amount_at(i: u32) -> f32 {
    var k = params.k;
    if params.per_pixel != 0u {
        k = amount[i];
    }
    return clamp(k, 0.0, 1.0);
}

// `Pixels::lay`: `src` laid on `dst` by `params.mode`, scaled by the amount.
@compute @workgroup_size(256)
fn lay(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let s = src[i];
    let a = clamp(s.a, 0.0, 1.0);
    let k = amount_at(i);
    // A stencil or silhouette changes every px, also where nothing is laid.
    if (a <= 0.0 || k <= 0.0) && !keeps_below(params.mode) {
        return;
    }
    let within = vec4(clamp(s.rgb, vec3(0.0), vec3(a)) * k, a * k);
    dst[i] = lay_one(params.mode, dst[i], within);
}

// `Pixels::mix`: `dst` moved towards `src` by the amount.
@compute @workgroup_size(256)
fn mix_towards(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let t = amount_at(i);
    let p = dst[i];
    dst[i] = p + (src[i] - p) * t;
}

// A cut: every channel times the amount kept, one value a pixel.
@compute @workgroup_size(256)
fn cut(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    dst[i] = dst[i] * amount[i];
}

// A moving effect's opacity: every channel times `k`.
@compute @workgroup_size(256)
fn scale(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    dst[i] = dst[i] * params.k;
}

// `glow`: a pixel whose unpremultiplied luma is below the threshold gives
// no light.
@compute @workgroup_size(256)
fn glow_threshold(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let p = dst[i];
    var luma = 0.0;
    if p.a > 0.0 {
        luma = (p.r * params.colour.r + p.g * params.colour.g + p.b * params.colour.b) / p.a;
    }
    if luma < params.threshold {
        dst[i] = vec4(0.0);
    }
}

// `drop-shadow`: the alpha times the opacity, in the shadow's colour.
@compute @workgroup_size(256)
fn shadow_colour(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let a = dst[i].a * params.opacity;
    dst[i] = vec4(params.colour.rgb * a, a);
}

// `drop-shadow`: the layer `dst` over its shadow `src`.
@compute @workgroup_size(256)
fn shadow_under(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let p = dst[i];
    dst[i] = p + src[i] * (1.0 - p.a);
}
