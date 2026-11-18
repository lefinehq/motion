// Per-pixel kernels of the float canvas: laying by a blend mode, mixing,
// cutting, scaling, and the per-pixel steps of the glow and the shadow.
// Each ports one CPU function of `nakkash-raster`, named beside it; pixels
// are premultiplied RGBA floats, as `Pixels` holds them.
//
// The blend modes' `MODE_*` constants are written in front of this source
// from the rasteriser's own table, so a mode's number is its row there.

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

// --- Blend modes: `blend.rs`, the W3C compositing and blending module ---

fn screen_one(b: f32, s: f32) -> f32 {
    return b + s - b * s;
}

fn hard_light_one(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        return b * 2.0 * s;
    }
    return screen_one(b, 2.0 * s - 1.0);
}

fn color_dodge_one(b: f32, s: f32) -> f32 {
    if b <= 0.0 {
        return 0.0;
    }
    if s >= 1.0 {
        return 1.0;
    }
    return min(b / (1.0 - s), 1.0);
}

fn color_burn_one(b: f32, s: f32) -> f32 {
    if b >= 1.0 {
        return 1.0;
    }
    if s <= 0.0 {
        return 0.0;
    }
    return 1.0 - min((1.0 - b) / s, 1.0);
}

fn soft_light_one(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        return b - (1.0 - 2.0 * s) * b * (1.0 - b);
    }
    var d = sqrt(b);
    if b <= 0.25 {
        d = ((16.0 * b - 12.0) * b + 4.0) * b;
    }
    return b + (2.0 * s - 1.0) * (d - b);
}

fn separable(mode: u32, b: f32, s: f32) -> f32 {
    switch mode {
        case MODE_MULTIPLY: { return b * s; }
        case MODE_SCREEN: { return screen_one(b, s); }
        case MODE_OVERLAY: { return hard_light_one(s, b); }
        case MODE_DARKEN: { return min(b, s); }
        case MODE_LIGHTEN: { return max(b, s); }
        case MODE_COLOR_DODGE: { return color_dodge_one(b, s); }
        case MODE_COLOR_BURN: { return color_burn_one(b, s); }
        case MODE_HARD_LIGHT: { return hard_light_one(b, s); }
        case MODE_SOFT_LIGHT: { return soft_light_one(b, s); }
        case MODE_DIFFERENCE: { return abs(b - s); }
        case MODE_EXCLUSION: { return b + s - 2.0 * b * s; }
        default: { return s; }
    }
}

fn lum(c: vec3<f32>) -> f32 {
    return 0.3 * c.r + 0.59 * c.g + 0.11 * c.b;
}

fn clip_color(color: vec3<f32>) -> vec3<f32> {
    let l = lum(color);
    let n = min(min(color.r, color.g), color.b);
    let x = max(max(color.r, color.g), color.b);
    var c = color;
    if n < 0.0 {
        c = vec3(l) + (c - vec3(l)) * l / (l - n);
    }
    if x > 1.0 {
        c = vec3(l) + (c - vec3(l)) * (1.0 - l) / (x - l);
    }
    return c;
}

fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    let d = l - lum(c);
    return clip_color(c + vec3(d));
}

fn sat(c: vec3<f32>) -> f32 {
    return max(max(c.r, c.g), c.b) - min(min(c.r, c.g), c.b);
}

fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let n = min(min(c.r, c.g), c.b);
    let x = max(max(c.r, c.g), c.b);
    if x > n {
        return (c - vec3(n)) * s / (x - n);
    }
    return vec3(0.0);
}

fn mixed(mode: u32, b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    switch mode {
        case MODE_NORMAL: { return s; }
        case MODE_HUE: { return set_lum(set_sat(s, sat(b)), lum(b)); }
        case MODE_SATURATION: { return set_lum(set_sat(b, sat(s)), lum(b)); }
        case MODE_COLOR: { return set_lum(s, lum(b)); }
        case MODE_LUMINOSITY: { return set_lum(b, lum(s)); }
        default: {
            return vec3(separable(mode, b.r, s.r), separable(mode, b.g, s.g), separable(mode, b.b, s.b));
        }
    }
}

fn unmultiplied(p: vec4<f32>, a: f32) -> vec3<f32> {
    if a > 0.0 {
        return clamp(p.rgb / a, vec3(0.0), vec3(1.0));
    }
    return vec3(0.0);
}

// `BlendMode::lay`: `source` laid on `below`, both premultiplied.
fn lay_one(mode: u32, below: vec4<f32>, source: vec4<f32>) -> vec4<f32> {
    let source_alpha = source.a;
    let below_alpha = below.a;
    if mode == MODE_ADD {
        return min(below + source, vec4(1.0));
    }
    if mode == MODE_NORMAL {
        return source + below * (1.0 - source_alpha);
    }
    if source_alpha <= 0.0 {
        return below;
    }
    let m = mixed(mode, unmultiplied(below, below_alpha), unmultiplied(source, source_alpha));
    let rgb = source.rgb * (1.0 - below_alpha) + below.rgb * (1.0 - source_alpha) + source_alpha * below_alpha * m;
    return vec4(rgb, source_alpha + below_alpha * (1.0 - source_alpha));
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
    if a <= 0.0 {
        return;
    }
    let k = amount_at(i);
    if k <= 0.0 {
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

// `glow`: the spread light `src` times the intensity added to `dst`, each
// colour kept within its alpha.
@compute @workgroup_size(256)
fn glow_add(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let p = min(dst[i] + src[i] * params.intensity, vec4(1.0));
    dst[i] = vec4(min(p.rgb, vec3(p.a)), p.a);
}

// `glow` alone: the spread light `src` times the intensity, in place of
// `dst`.
@compute @workgroup_size(256)
fn glow_alone(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    dst[i] = min(src[i] * params.intensity, vec4(1.0));
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
