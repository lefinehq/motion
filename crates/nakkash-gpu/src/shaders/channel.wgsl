// The channel effects (`channel.rs`, `cycle.rs`): channels rebuilt from
// levels read off pixels, the layer's own or another layer's. Each kernel
// ports the CPU function named beside it; pixels are premultiplied RGBA
// floats.

struct Params {
    // How many pixels, how many invocations one row of workgroups covers,
    // and the grid's width and height.
    count: u32,
    row: u32,
    width: u32,
    height: u32,
    // Levels, by their number in `LEVEL_*`: set-matte's in `level0`;
    // shift-channels' alpha, red, green and blue; the colour cycle's phase
    // and added level.
    level0: u32,
    level1: u32,
    level2: u32,
    level3: u32,
    // How set-matte reads the matte's colour: `READ_*`.
    reading: u32,
    // Choices, as the bits `FLAG_*`.
    flags: u32,
    // The size of the grid `src` holds, for set-matte's matte.
    matte_width: u32,
    matte_height: u32,
    // The colour cycle's add mode and modify choice, as its plan holds
    // them, and how many points each of its four curves has.
    add_mode: u32,
    modify: u32,
    len0: u32,
    len1: u32,
    len2: u32,
    len3: u32,
    // set-matte: from the grid's px (column, row) to the point of the
    // matte sampled, less half a px: x = a·column + c·row + e,
    // y = b·column + d·row + f, as (a, b, c, d) and (e, f).
    place: vec4<f32>,
    shift: vec4<f32>,
    // The colour cycle's repetitions, its shift in turns, and its blend.
    cycle: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
// The matte, or the layer the colour cycle adds.
@group(0) @binding(2) var<storage, read> src: array<vec4<f32>>;
// The box's first and end column of each row; or the colour cycle's
// curves, 16 points of input and level each.
@group(0) @binding(3) var<storage, read> values: array<f32>;

// `Level`'s variants, in its order.
const LEVEL_RED: u32 = 0u;
const LEVEL_GREEN: u32 = 1u;
const LEVEL_BLUE: u32 = 2u;
const LEVEL_ALPHA: u32 = 3u;
const LEVEL_INTENSITY: u32 = 4u;
const LEVEL_VALUE: u32 = 5u;
const LEVEL_LUMINANCE: u32 = 6u;
const LEVEL_HUE: u32 = 7u;
const LEVEL_LIGHTNESS: u32 = 8u;
const LEVEL_SATURATION: u32 = 9u;
const LEVEL_FULL: u32 = 10u;
const LEVEL_OFF: u32 = 11u;

// `Reading`'s variants.
const READ_STRAIGHT: u32 = 0u;
const READ_OVER_BLACK: u32 = 1u;

// set-matte: the level inverted; composited, rather than replacing the
// alpha in the box.
const FLAG_INVERTED: u32 = 1u;
const FLAG_COMPOSITE: u32 = 2u;
// shift-channels: the alpha is full, so empty pixels in the box change.
const FLAG_FILLS: u32 = 4u;
// The colour cycle: a layer is added; the cycle runs straight between its
// points; its alpha multiplies the pixel's.
const FLAG_ADDED: u32 = 8u;
const FLAG_SMOOTH: u32 = 16u;
const FLAG_MODIFY_ALPHA: u32 = 32u;

fn index(gid: vec3<u32>) -> u32 {
    return gid.y * params.row + gid.x;
}

fn flag(bit: u32) -> bool {
    return (params.flags & bit) != 0u;
}

// `f32::rem_euclid`: `x` wrapped into 0 to `y`, for `y` above 0.
fn rem_euclid(x: f32, y: f32) -> f32 {
    let r = x - y * trunc(x / y);
    if r < 0.0 {
        return r + y;
    }
    return r;
}

// `x / y` rounded as the CPU rounds it: the hardware's quotient, which
// may be a step or two off, corrected once by its remainder, which a fused
// multiply-add works out exactly. A quotient that is not finite (`y` is 0,
// or it overflows) is left as it is.
fn quotient(x: f32, y: f32) -> f32 {
    let q = x / y;
    if (bitcast<u32>(q) & 0x7f800000u) == 0x7f800000u {
        return q;
    }
    let r = fma(-q, y, x);
    return fma(r, 1.0 / y, q);
}

// Whether pixel `i` lies in the box: within its row's span in `values`.
fn in_box(i: u32) -> bool {
    let x = f32(i % params.width);
    let y = i / params.width;
    return x >= values[2u * y] && x < values[2u * y + 1u];
}

// `Level::of`: the level of the premultiplied `p`, its colour read as
// `reading` says, 0 to 1.
fn level_of(level: u32, p: vec4<f32>, reading: u32) -> f32 {
    var r = p.r;
    var g = p.g;
    var b = p.b;
    let a = p.a;
    if reading == READ_STRAIGHT {
        if a > 0.0 {
            r = quotient(p.r, a);
            g = quotient(p.g, a);
            b = quotient(p.b, a);
        } else {
            r = 0.0;
            g = 0.0;
            b = 0.0;
        }
    }
    let mx = max(max(r, g), b);
    let mn = min(min(r, g), b);
    var out = 0.0;
    switch level {
        case LEVEL_RED: { out = r; }
        case LEVEL_GREEN: { out = g; }
        case LEVEL_BLUE: { out = b; }
        case LEVEL_ALPHA: { out = a; }
        case LEVEL_INTENSITY: { out = quotient(r + g + b, 3.0); }
        case LEVEL_VALUE: { out = mx; }
        case LEVEL_LUMINANCE: { out = r * 0.299 + g * 0.587 + b * 0.114; }
        case LEVEL_HUE: {
            let d = mx - mn;
            if d <= 0.0 {
                out = 0.0;
            } else if mx == r {
                out = quotient(rem_euclid(quotient(g - b, d), 6.0), 6.0);
            } else if mx == g {
                out = quotient(quotient(b - r, d) + 2.0, 6.0);
            } else {
                out = quotient(quotient(r - g, d) + 4.0, 6.0);
            }
        }
        case LEVEL_LIGHTNESS: { out = (mx + mn) / 2.0; }
        case LEVEL_SATURATION: {
            let l = (mx + mn) / 2.0;
            let d = mx - mn;
            if d <= 0.0 {
                out = 0.0;
            } else {
                out = quotient(d, max(1.0 - abs(2.0 * l - 1.0), 1e-6));
            }
        }
        case LEVEL_FULL: { out = 1.0; }
        default: { out = 0.0; }
    }
    return clamp(out, 0.0, 1.0);
}

// `Pixels::get` of the matte: transparent beyond its edges.
fn matte_at(x: i32, y: i32) -> vec4<f32> {
    if x < 0 || y < 0 || x >= i32(params.matte_width) || y >= i32(params.matte_height) {
        return vec4(0.0);
    }
    return src[u32(y) * params.matte_width + u32(x)];
}

// `Pixels::sample`'s mix: `a` moved towards `b` by `t`.
fn towards(a: vec4<f32>, b: vec4<f32>, t: f32) -> vec4<f32> {
    return a + (b - a) * t;
}

// `set-matte`: the layer's alpha from a level of the matte at the same
// place.
@compute @workgroup_size(256)
fn set_matte(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let composite = flag(FLAG_COMPOSITE);
    if !composite && !in_box(i) {
        return;
    }
    let column = f32(i % params.width);
    let row = f32(i / params.width);
    // `Pixels::sample` at the matte's point, already half a px back.
    let x = params.place.x * column + params.place.z * row + params.shift.x;
    let y = params.place.y * column + params.place.w * row + params.shift.y;
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let c0 = i32(x0);
    let r0 = i32(y0);
    let top = towards(matte_at(c0, r0), matte_at(c0 + 1, r0), fx);
    let bottom = towards(matte_at(c0, r0 + 1), matte_at(c0 + 1, r0 + 1), fx);
    var m = level_of(params.level0, towards(top, bottom, fy), params.reading);
    if flag(FLAG_INVERTED) {
        m = 1.0 - m;
    }
    let p = dst[i];
    if composite {
        dst[i] = p * m;
        return;
    }
    let a = p.a;
    var s = vec3(0.0);
    if a > 0.0 {
        s = vec3(quotient(p.r, a), quotient(p.g, a), quotient(p.b, a));
    }
    dst[i] = vec4(s * m, m);
}

// `shift-channels`: each channel a level of the straight colour and alpha
// as they were.
@compute @workgroup_size(256)
fn shift_channels(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let was = dst[i];
    if was.a <= 0.0 && (!flag(FLAG_FILLS) || !in_box(i)) {
        return;
    }
    let a = level_of(params.level0, was, READ_STRAIGHT);
    let r = level_of(params.level1, was, READ_STRAIGHT);
    let g = level_of(params.level2, was, READ_STRAIGHT);
    let b = level_of(params.level3, was, READ_STRAIGHT);
    dst[i] = vec4(r * a, g * a, b * a, a);
}

// Point `k` of curve `c` of the cycle: its input and level.
fn point(c: u32, k: u32) -> vec2<f32> {
    let at = (c * 16u + k) * 2u;
    return vec2(values[at], values[at + 1u]);
}

// How many points curve `c` of the cycle has.
fn points(c: u32) -> u32 {
    switch c {
        case 0u: { return params.len0; }
        case 1u: { return params.len1; }
        case 2u: { return params.len2; }
        default: { return params.len3; }
    }
}

// `level_at`: the level of the cyclic curve `c` at phase `t`, `empty` for
// a curve of no points.
fn level_at(c: u32, t: f32, joined: bool, empty: f32) -> f32 {
    let n = points(c);
    if n == 0u {
        return empty;
    }
    let first = point(c, 0u);
    let last = point(c, n - 1u);
    var next = n;
    for (var k = 0u; k < n; k++) {
        if point(c, k).x > t {
            next = k;
            break;
        }
    }
    var start = vec2(last.x, last.y);
    var stop = vec2(first.x + 1.0, first.y);
    if next < n && next > 0u {
        start = point(c, next - 1u);
        stop = point(c, next);
    }
    let span = stop.x - start.x;
    if joined && span > 0.0 {
        return start.y + quotient((stop.y - start.y) * rem_euclid(t - start.x, 1.0), span);
    }
    return start.y;
}

// `to_hls`: a straight colour as hue, lightness and saturation.
fn to_hls(c: vec3<f32>) -> vec3<f32> {
    let p = vec4(c, 1.0);
    return vec3(
        level_of(LEVEL_HUE, p, READ_STRAIGHT),
        level_of(LEVEL_LIGHTNESS, p, READ_STRAIGHT),
        level_of(LEVEL_SATURATION, p, READ_STRAIGHT),
    );
}

// `from_hls`: hue, lightness and saturation as a straight colour.
fn from_hls(hls: vec3<f32>) -> vec3<f32> {
    let h = hls.x;
    let l = hls.y;
    let s = hls.z;
    let c = (1.0 - abs(2.0 * l - 1.0)) * s;
    let h6 = rem_euclid(h, 1.0) * 6.0;
    let x = c * (1.0 - abs(rem_euclid(h6, 2.0) - 1.0));
    var rgb: vec3<f32>;
    switch u32(h6) {
        case 0u: { rgb = vec3(c, x, 0.0); }
        case 1u: { rgb = vec3(x, c, 0.0); }
        case 2u: { rgb = vec3(0.0, c, x); }
        case 3u: { rgb = vec3(0.0, x, c); }
        case 4u: { rgb = vec3(x, 0.0, c); }
        default: { rgb = vec3(c, 0.0, x); }
    }
    let m = l - c / 2.0;
    return vec3(rgb.r + m, rgb.g + m, rgb.b + m);
}

// The channels each `modify` choice replaces: red, green and blue for 0
// to 6, then hue, lightness and saturation for 7 to 12.
const TAKES: array<vec3<bool>, 13> = array<vec3<bool>, 13>(
    vec3(true, true, true),
    vec3(true, false, false),
    vec3(false, true, false),
    vec3(false, false, true),
    vec3(true, true, false),
    vec3(false, true, true),
    vec3(true, false, true),
    vec3(true, false, false),
    vec3(false, true, false),
    vec3(false, false, true),
    vec3(true, true, false),
    vec3(false, true, true),
    vec3(true, false, true),
);

// `color-cycle`: each pixel's phase sent round the cycle, whose colour
// replaces some of its channels.
@compute @workgroup_size(256)
fn color_cycle(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let p = dst[i];
    let alpha = p.a;
    if alpha <= 0.0 {
        return;
    }
    var phase = level_of(params.level0, p, READ_STRAIGHT);
    if flag(FLAG_ADDED) {
        let q = level_of(params.level1, src[i], READ_STRAIGHT);
        switch params.add_mode {
            case 0u: { phase = rem_euclid(phase + q, 1.0); }
            case 1u: { phase = min(phase + q, 1.0); }
            case 2u: { phase = (phase + q) / 2.0; }
            default: { phase = 1.0 - (1.0 - phase) * (1.0 - q); }
        }
    }
    let t = rem_euclid(phase * params.cycle.x + params.cycle.y, 1.0);
    let joined = flag(FLAG_SMOOTH);
    // An empty curve is the phase itself in a colour, opaque in alpha.
    let cr = clamp(level_at(0u, t, joined, t), 0.0, 1.0);
    let cg = clamp(level_at(1u, t, joined, t), 0.0, 1.0);
    let cb = clamp(level_at(2u, t, joined, t), 0.0, 1.0);
    let ca = clamp(level_at(3u, t, joined, 1.0), 0.0, 1.0);
    let was = vec3(quotient(p.r, alpha), quotient(p.g, alpha), quotient(p.b, alpha));
    let cycled = vec3(cr, cg, cb);
    var rgb = was;
    let modify = params.modify;
    if modify <= 6u {
        rgb = select(was, cycled, TAKES[modify]);
    } else if modify <= 12u {
        rgb = from_hls(select(to_hls(was), to_hls(cycled), TAKES[modify]));
    }
    var a = alpha;
    if flag(FLAG_MODIFY_ALPHA) {
        a = alpha * ca;
    }
    let blend = params.cycle.z;
    let mixed = rgb + (was - rgb) * blend;
    let kept = a + (alpha - a) * blend;
    dst[i] = vec4(mixed * kept, kept);
}
