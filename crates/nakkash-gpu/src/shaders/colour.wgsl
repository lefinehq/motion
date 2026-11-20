// The colour effects (`colour.rs`) and the fill: each changes every pixel
// on its own. Each kernel ports the CPU function named beside it; pixels
// are premultiplied RGBA floats. Most work on a pixel's straight colour,
// as `each` gives it: a pixel that is not transparent is divided by its
// alpha, changed, held to 0 to 1 and multiplied by its alpha again, and a
// transparent one stays as it is.

struct Params {
    // How many pixels, how many invocations one row of workgroups covers,
    // and the grid's width and height.
    count: u32,
    row: u32,
    width: u32,
    height: u32,
    // The channels changed, red 1, green 2, blue 4 and alpha 8; for the
    // curves, the tables there are, `rgb` 1, red 2, green 4, blue 8 and
    // alpha 16.
    channels: u32,
    // A choice: colorize for the hue and saturation, linear for the
    // exposure, inverted for the fill; for the brightness and contrast,
    // curves with no brightness 0, brightening 1, darkening 2, a legacy
    // line 3 and a legacy threshold 4.
    choice: u32,
    // Whether an inverted fill is held to its box, a span of each row in
    // `table`.
    boxed: u32,
    // Numbers, by kernel:
    // - tint: amount;
    // - hue_saturation: the turn or hue, saturation, lightness;
    // - exposure: scale, offset, 1/gamma, 1/2.4;
    // - brightness_contrast: contrast, power, shape; legacy: offset, slope;
    // - levels: input black, input white, output black, output white;
    // - invert: blend;
    // - black_white: the tint colour's hue and saturation, then tint;
    // - fill: share.
    numbers: vec4<f32>,
    // levels: gamma and 1/gamma; brightness_contrast: 1/power, 1/shape.
    more: vec4<f32>,
    // tint: black; black_white: the first four shares; fill: the colour.
    first: vec4<f32>,
    // tint: white; black_white: the last two shares.
    second: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
// The curves' tables, 256 levels each in the order of `channels`' bits;
// or the fill's box, the first and the end column of each row.
@group(0) @binding(2) var<storage, read> table: array<f32>;

fn index(gid: vec3<u32>) -> u32 {
    return gid.y * params.row + gid.x;
}

// The smallest step above 1 of an `f32`, as `f32::EPSILON`.
const EPSILON: f32 = 1.1920929e-7;

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

// `each`: the straight colour of `p`, whose alpha is above 0.
fn straight_of(p: vec4<f32>) -> vec4<f32> {
    return vec4(quotient(p.r, p.a), quotient(p.g, p.a), quotient(p.b, p.a), p.a);
}

// `each`: a changed straight colour held to 0 to 1 and premultiplied.
fn premultiplied(c: vec4<f32>) -> vec4<f32> {
    let h = clamp(c, vec4(0.0), vec4(1.0));
    return vec4(h.r * h.a, h.g * h.a, h.b * h.a, h.a);
}

// Whether channel `c` (red 0 to alpha 3) is one the kernel changes.
fn changes(c: u32) -> bool {
    return (params.channels & (1u << c)) != 0u;
}

// `CurveTable::read`: the level `c` maps to in table `t`, read linearly
// between its steps.
fn table_read(t: u32, c: f32) -> f32 {
    let x = clamp(c, 0.0, 1.0) * 255.0;
    let i = min(u32(floor(x)), 254u);
    let f = x - f32(i);
    let lo = table[t * 256u + i];
    let hi = table[t * 256u + i + 1u];
    return lo + (hi - lo) * f;
}

// `c` through table `t`, or as it is where there is none.
fn through(t: u32, c: f32) -> f32 {
    if (params.channels & (1u << t)) != 0u {
        return table_read(t, c);
    }
    return c;
}

// `curves`: each colour through its channel's table, then `rgb`'s; alpha
// through its own.
@compute @workgroup_size(256)
fn curves(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count || dst[i].a <= 0.0 {
        return;
    }
    let s = straight_of(dst[i]);
    dst[i] = premultiplied(vec4(
        through(0u, through(1u, s.r)),
        through(0u, through(2u, s.g)),
        through(0u, through(3u, s.b)),
        through(4u, s.a),
    ));
}

// `tint`: each colour's luma picks a colour between black and white,
// which replaces it by the amount.
@compute @workgroup_size(256)
fn tint(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count || dst[i].a <= 0.0 {
        return;
    }
    let s = straight_of(dst[i]);
    let luma = s.r * 0.299 + s.g * 0.587 + s.b * 0.114;
    let black = params.first.rgb;
    let white = params.second.rgb;
    let tinted = black + (white - black) * luma;
    let amount = params.numbers.x;
    dst[i] = premultiplied(vec4(s.rgb + (tinted - s.rgb) * amount, s.a));
}

// `to_hsl`: hue, saturation and lightness of `c`, each 0 to 1.
fn to_hsl(c: vec3<f32>) -> vec3<f32> {
    let r = c.r;
    let g = c.g;
    let b = c.b;
    let mx = max(max(r, g), b);
    let mn = min(min(r, g), b);
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    if d <= EPSILON {
        return vec3(0.0, 0.0, l);
    }
    var s: f32;
    if l > 0.5 {
        s = quotient(d, 2.0 - mx - mn);
    } else {
        s = quotient(d, mx + mn);
    }
    var h: f32;
    if mx == r {
        var lift = 0.0;
        if g < b {
            lift = 6.0;
        }
        h = quotient(g - b, d) + lift;
    } else if mx == g {
        h = quotient(b - r, d) + 2.0;
    } else {
        h = quotient(r - g, d) + 4.0;
    }
    return vec3(quotient(h, 6.0), s, l);
}

// `from_hsl`: one channel of the colour at hue `t`.
fn hsl_channel(t_in: f32, p: f32, q: f32) -> f32 {
    let t = rem_euclid(t_in, 1.0);
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    } else if t < 0.5 {
        return q;
    } else if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    return p;
}

// `from_hsl`: the RGB of a hue, saturation and lightness, each 0 to 1.
fn from_hsl(hsl: vec3<f32>) -> vec3<f32> {
    let h = hsl.x;
    let s = hsl.y;
    let l = hsl.z;
    if s <= 0.0 {
        return vec3(l);
    }
    var q: f32;
    if l < 0.5 {
        q = l * (1.0 + s);
    } else {
        q = l + s - l * s;
    }
    let p = 2.0 * l - q;
    return vec3(
        hsl_channel(h + 1.0 / 3.0, p, q),
        hsl_channel(h, p, q),
        hsl_channel(h - 1.0 / 3.0, p, q),
    );
}

// `hue_saturation`: `value` moved towards 0 by a share below 0, towards 1
// by one above.
fn towards(value: f32, by: f32) -> f32 {
    if by < 0.0 {
        return value * (1.0 + by);
    }
    return value + (1.0 - value) * by;
}

// `hue-saturation`: hues turned, saturation and lightness moved; or, with
// colorize, one hue and saturation at each colour's luma.
@compute @workgroup_size(256)
fn hue_saturation(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count || dst[i].a <= 0.0 {
        return;
    }
    let s = straight_of(dst[i]);
    let n = params.numbers;
    var turned: vec3<f32>;
    if params.choice == 0u {
        let hsl = to_hsl(s.rgb);
        turned = from_hsl(vec3(rem_euclid(hsl.x + n.x, 1.0), towards(hsl.y, n.y), hsl.z));
    } else {
        let luma = s.r * 0.299 + s.g * 0.587 + s.b * 0.114;
        turned = from_hsl(vec3(n.x, n.y, luma));
    }
    let lit = vec3(towards(turned.r, n.z), towards(turned.g, n.z), towards(turned.b, n.z));
    dst[i] = premultiplied(vec4(lit, s.a));
}

// `decode`: sRGB's transfer function, from encoded values to light.
fn decode(c: f32) -> f32 {
    if c <= 0.04045 {
        return quotient(c, 12.92);
    }
    return pow(quotient(c + 0.055, 1.055), 2.4);
}

// `encode`: sRGB's transfer function, from light to encoded values.
fn encode(light: f32) -> f32 {
    if light <= 0.0031308 {
        return light * 12.92;
    }
    return 1.055 * pow(light, params.numbers.w) - 0.055;
}

// `exposure`: one level's light scaled, offset and raised to 1/gamma.
fn exposed(c: f32) -> f32 {
    let n = params.numbers;
    var light = c;
    if params.choice != 0u {
        light = decode(c);
    }
    light = pow(max(light * n.x + n.y, 0.0), n.z);
    if params.choice != 0u {
        return encode(light);
    }
    return light;
}

// `exposure`: every colour's light scaled, offset and raised to 1/gamma.
@compute @workgroup_size(256)
fn exposure(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count || dst[i].a <= 0.0 {
        return;
    }
    let s = straight_of(dst[i]);
    dst[i] = premultiplied(vec4(exposed(s.r), exposed(s.g), exposed(s.b), s.a));
}

// `BrightnessContrast::level`: what one straight level becomes.
fn brightened(c: f32) -> f32 {
    let n = params.numbers;
    if params.choice >= 3u {
        let from_grey = c + n.x - 0.5;
        if params.choice == 4u {
            return select(0.0, 1.0, from_grey >= 0.0);
        }
        return from_grey * n.y + 0.5;
    }
    var x = clamp(c, 0.0, 1.0);
    x = clamp(x + n.x * x * (1.0 - x) * (2.0 * x - 1.0), 0.0, 1.0);
    if params.choice == 1u {
        return 1.0 - pow(max(1.0 - pow(x, n.z), 0.0), n.y);
    }
    if params.choice == 2u {
        let m = params.more;
        return pow(max(1.0 - pow(max(1.0 - x, 0.0), m.x), 0.0), m.y);
    }
    return x;
}

// `brightness-contrast`: every colour level bent by the curves or moved
// along the legacy line.
@compute @workgroup_size(256)
fn brightness_contrast(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count || dst[i].a <= 0.0 {
        return;
    }
    let s = straight_of(dst[i]);
    dst[i] = premultiplied(vec4(brightened(s.r), brightened(s.g), brightened(s.b), s.a));
}

// `levels`: one level mapped between the ranges through the gamma.
fn levelled(c: f32) -> f32 {
    let n = params.numbers;
    let span = n.y - n.x;
    var t: f32;
    if abs(span) > EPSILON {
        t = clamp(quotient(c - n.x, span), 0.0, 1.0);
    } else {
        t = select(0.0, 1.0, c >= n.y);
    }
    if params.more.x > 0.0 {
        t = pow(t, params.more.y);
    } else {
        t = 0.0;
    }
    return n.z + (n.w - n.z) * t;
}

// `levels`: the levels of the channels mapped.
@compute @workgroup_size(256)
fn levels(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count || dst[i].a <= 0.0 {
        return;
    }
    var s = straight_of(dst[i]);
    for (var c = 0u; c < 4u; c++) {
        if changes(c) {
            s[c] = levelled(s[c]);
        }
    }
    dst[i] = premultiplied(s);
}

// `invert`: the levels of the channels turned into 1 minus themselves,
// with the blend of the original kept.
@compute @workgroup_size(256)
fn invert(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count || dst[i].a <= 0.0 {
        return;
    }
    let blend = params.numbers.x;
    var s = straight_of(dst[i]);
    for (var c = 0u; c < 4u; c++) {
        if changes(c) {
            s[c] = (1.0 - s[c]) * (1.0 - blend) + s[c] * blend;
        }
    }
    dst[i] = premultiplied(s);
}

// `grey`: the grey of `c` under the six hues' shares.
fn grey(c: vec3<f32>) -> f32 {
    let share = array<f32, 6>(
        params.first.x,
        params.first.y,
        params.first.z,
        params.first.w,
        params.second.x,
        params.second.y,
    );
    // Each channel's primary share.
    let primary = array<u32, 3>(0u, 2u, 4u);
    let levels = array<f32, 3>(c.r, c.g, c.b);
    // The channels from the highest level to the lowest, ties in their
    // own order, as a stable sort leaves them.
    var order = array<u32, 3>(0u, 1u, 2u);
    for (var k = 1u; k < 3u; k++) {
        let moved = order[k];
        var j = k;
        while j > 0u && levels[order[j - 1u]] < levels[moved] {
            order[j] = order[j - 1u];
            j--;
        }
        order[j] = moved;
    }
    let high = order[0];
    let middle = order[1];
    let low = order[2];
    // The secondary share of the pair the two highest make.
    var secondary = 5u;
    let pair = vec2(min(high, middle), max(high, middle));
    if pair.x == 0u && pair.y == 1u {
        secondary = 1u;
    } else if pair.x == 1u && pair.y == 2u {
        secondary = 3u;
    }
    return levels[low]
        + (levels[high] - levels[middle]) * share[primary[high]]
        + (levels[middle] - levels[low]) * share[secondary];
}

// `black-white`: each colour's grey under the hues' shares, tinted.
@compute @workgroup_size(256)
fn black_white(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count || dst[i].a <= 0.0 {
        return;
    }
    let s = straight_of(dst[i]);
    let g = clamp(grey(s.rgb), 0.0, 1.0);
    let tinted = from_hsl(vec3(params.numbers.x, params.numbers.y, g));
    let tint = params.numbers.z;
    dst[i] = premultiplied(vec4(g + (tinted - g) * tint, s.a));
}

// `fill_pixels`: the colour laid "source-atop" every pixel, or, inverted,
// "xor" every pixel of the box.
@compute @workgroup_size(256)
fn fill(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let colour = params.first.rgb;
    let share = params.numbers.x;
    var p = dst[i];
    if params.choice == 0u {
        let alpha = p.a;
        dst[i] = vec4(colour * share * alpha + p.rgb * (1.0 - share), alpha);
        return;
    }
    if params.boxed != 0u {
        let x = f32(i % params.width);
        let y = i / params.width;
        if x < table[2u * y] || x >= table[2u * y + 1u] {
            return;
        }
    }
    let clear = 1.0 - p.a;
    dst[i] = vec4(colour * share * clear + p.rgb * (1.0 - share), share * clear + p.a * (1.0 - share));
}
