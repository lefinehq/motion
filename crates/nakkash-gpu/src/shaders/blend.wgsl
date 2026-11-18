// The blend modes, as `blend.rs` works them out: the W3C compositing and
// blending module. Written in front of every family that lays a pixel by a
// mode, after the `MODE_*` constants.

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

