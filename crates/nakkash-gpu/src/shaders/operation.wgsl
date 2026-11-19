// Compositing inside effects, as `composite.rs` does it: an effect's
// result combined with the layer's pixels by one of the operations
// (`OPERATIONS`). Written in front of every family that combines by an
// operation, after the blend modes (`blend.wgsl`).

// An operation's number, by its variant of `Operation`.
const OP_REPLACE: u32 = 0u;
const OP_BLEND: u32 = 1u;
const OP_DISSOLVE: u32 = 2u;
const OP_STENCIL_ALPHA: u32 = 3u;
const OP_STENCIL_LUMA: u32 = 4u;
const OP_SILHOUETTE_ALPHA: u32 = 5u;
const OP_SILHOUETTE_LUMA: u32 = 6u;
const OP_PREMULTIPLIED_OVER: u32 = 7u;
const OP_ALPHA_ADD: u32 = 8u;

// `held`: a premultiplied pixel held to 0 to 1, its colours to its alpha.
fn held(p: vec4<f32>) -> vec4<f32> {
    let a = clamp(p.a, 0.0, 1.0);
    return vec4(clamp(p.rgb, vec3(0.0), vec3(a)), a);
}

// `straight`: the straight colour of a premultiplied pixel.
fn straight(p: vec4<f32>) -> vec3<f32> {
    if p.a > 0.0 {
        return p.rgb / p.a;
    }
    return vec3(0.0);
}

// `luma`: the straight luma times the alpha, by Rec. 601's weights
// (`LUMA`).
fn luma(p: vec4<f32>) -> f32 {
    return p.r * 0.299 + p.g * 0.587 + p.b * 0.114;
}

// `dither`: an integer hash of the pixel's position, 0 to 1.
fn dither(x: u32, y: u32) -> f32 {
    let y_mixed = y * 0x85EBCA77u;
    var h = (x * 0x9E3779B1u) ^ ((y_mixed << 16u) | (y_mixed >> 16u));
    h ^= h >> 15u;
    h *= 0x2C1B3C6Du;
    h ^= h >> 12u;
    h *= 0x297A2D39u;
    h ^= h >> 15u;
    return f32(h >> 8u) / 16777216.0;
}

// `Operation::pixel`: `over` combined with `under` by the operation `op`
// (and blend `mode`), at pixel `i` of a grid `width` px wide.
fn operate_one(op: u32, mode: u32, width_in: u32, under_in: vec4<f32>, over_in: vec4<f32>, i: u32) -> vec4<f32> {
    let under = held(under_in);
    let lit = clamp(over_in, vec4(0.0), vec4(1.0));
    let over = held(over_in);
    let ab = under.a;
    let a_s = over.a;
    var out = over;
    switch op {
        case OP_REPLACE: { out = over; }
        case OP_BLEND: { out = lay_one(mode, under, over); }
        case OP_DISSOLVE: {
            let width = max(width_in, 1u);
            if dither(i % width, i / width) < a_s {
                out = vec4(straight(over), 1.0);
            } else {
                out = under;
            }
        }
        case OP_STENCIL_ALPHA: { out = under * a_s; }
        case OP_STENCIL_LUMA: { out = under * luma(over); }
        case OP_SILHOUETTE_ALPHA: { out = under * (1.0 - a_s); }
        case OP_SILHOUETTE_LUMA: { out = under * (1.0 - luma(over)); }
        case OP_PREMULTIPLIED_OVER: { out = lit + under * (1.0 - lit.a); }
        case OP_ALPHA_ADD: {
            var share = 0.0;
            if ab > 0.0 {
                share = min((1.0 - a_s) / ab, 1.0);
            }
            out = over + under * share;
        }
        default: { out = over; }
    }
    return held(out);
}

