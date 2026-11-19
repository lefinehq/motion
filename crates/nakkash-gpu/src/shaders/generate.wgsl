// The generators of `generate.rs` but fractal noise (`noise.wgsl`): the
// ramp, four colours, the grid, the venetian blinds, the linear and iris
// wipes, and the beam. Each kernel ports the CPU function named beside it,
// reading its plan from the parameters and `lines`.
//
// Where a pixel's result jumps (a stripe's edge, a wipe's hard edge, the
// edge of the layer's box), the steps that place it are rounded as the
// CPU rounds them: each product on its own (`product`), sums in the CPU's
// order (`then`), divisions to the nearest (`divided`). Left to itself the
// GPU fuses a product into the addition after it and divides by a
// reciprocal, each a bit apart.
//
// The operations (`operation.wgsl`) and the layer's points (`layer.wgsl`)
// are written in front of this source.

struct Params {
    // How many pixels, how many invocations one row of workgroups covers,
    // and the grid's size.
    count: u32,
    row: u32,
    width: u32,
    height: u32,
    // The operation and its blend mode, for the generators that combine
    // with the layer by one.
    op: u32,
    mode: u32,
    // Whether `lines` holds the layer's point at every pixel, x then y,
    // rather than the layer's lines; where a kernel's own values start in
    // it.
    points: u32,
    extra: u32,
    // A count and a choice, named by each kernel.
    n: u32,
    flag: u32,
    // 0 and 1, which the kernel cannot see through and so cannot fold
    // into the steps they keep apart.
    unit: vec4<f32>,
    // Points, two to a vector, colours and numbers, named by each kernel.
    spots: array<vec4<f32>, 2>,
    colours: array<vec4<f32>, 4>,
    k: vec4<f32>,
    k2: vec4<f32>,
    // The pixels a kernel reaches, columns from x up to z and rows from y
    // up to w.
    range: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;

// `a * b` rounded on its own, as the CPU rounds a product before adding
// it to anything.
fn product(a: f32, b: f32) -> f32 {
    return fma(a, b, params.unit.x);
}

// `a + b`, rounded, before anything is added to it.
fn then(a: f32, b: f32) -> f32 {
    return fma(a, params.unit.y, b);
}

// `f32::rem_euclid` by `by` above 0: the remainder, which is exact, made
// positive.
fn rem_euclid(a: f32, by: f32) -> f32 {
    let r = a % by;
    if r < 0.0 {
        return r + by;
    }
    return r;
}

// A pixel the layer covers, as `each_in_layer` sees it.
struct Place {
    // Its index, whether the layer covers it, how much, its straight
    // colour and its centre's point in the layer.
    i: u32,
    covered: bool,
    coverage: f32,
    under: vec4<f32>,
    at: vec2<f32>,
}

fn place(gid: vec3<u32>) -> Place {
    var place: Place;
    place.i = gid.y * params.row + gid.x;
    place.covered = false;
    if place.i >= params.count {
        return place;
    }
    let pixel = dst[place.i];
    place.coverage = pixel.a;
    if place.coverage <= 0.0 {
        return place;
    }
    place.covered = true;
    place.under = straight_of(pixel, place.coverage);
    let w = max(params.width, 1u);
    if params.points != 0u {
        place.at = vec2(lines[2u * place.i], lines[2u * place.i + 1u]);
    } else {
        place.at = layer_point(place.i % w, place.i / w, params.width, params.height);
    }
    return place;
}

// `ramp`: spots[0] = start, direction; colours[0], colours[1] = the
// colours and alphas at its ends; k = length², blend; flag = radial.
@compute @workgroup_size(256)
fn ramp(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = place(gid);
    if !p.covered {
        return;
    }
    let start = params.spots[0].xy;
    let d = params.spots[0].zw;
    let length2 = params.k.x;
    let blend = params.k.y;
    let v = p.at - start;
    var t = 1.0;
    if length2 > 0.0 {
        if params.flag == 0u {
            t = (v.x * d.x + v.y * d.y) / length2;
        } else {
            t = sqrt((v.x * v.x + v.y * v.y) / length2);
        }
    }
    t = clamp(t, 0.0, 1.0);
    let start_colour = params.colours[0];
    let end_colour = params.colours[1];
    let c = start_colour.rgb + (end_colour.rgb - start_colour.rgb) * t;
    let a = start_colour.a + (end_colour.a - start_colour.a) * t;
    let mixed = c + (p.under.rgb - c) * blend;
    dst[p.i] = painted(vec4(mixed, a + (1.0 - a) * blend), p.coverage);
}

// `four_color`: spots = the four points; colours = their colours; k =
// the power, the opacity. The weights are taken against the nearest
// point's, the inverse of the distances' ratio to the power: the same
// mean, without the tiny floats the GPU would flush to zero where the CPU
// keeps them, and without the large logarithms its power would round.
@compute @workgroup_size(256)
fn four_color(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = place(gid);
    if !p.covered {
        return;
    }
    let points = array(params.spots[0].xy, params.spots[0].zw, params.spots[1].xy, params.spots[1].zw);
    let exponent = -params.k.x / 2.0;
    var d2: array<f32, 4>;
    var at = 4u;
    var nearest = 3.4e38;
    for (var j = 0u; j < 4u; j++) {
        let d = p.at - points[j];
        d2[j] = d.x * d.x + d.y * d.y;
        if d2[j] > 1e-6 {
            nearest = min(nearest, d2[j]);
        } else if at == 4u {
            at = j;
        }
    }
    var c: vec3<f32>;
    if at < 4u {
        c = params.colours[at].rgb;
    } else {
        var total = 0.0;
        var sum = vec3(0.0);
        for (var j = 0u; j < 4u; j++) {
            let w = pow(d2[j] / nearest, exponent);
            total += w;
            sum += w * params.colours[j].rgb;
        }
        c = sum / total;
    }
    let w = max(params.width, 1u);
    dst[p.i] = painted(composite(params.op, params.mode, w, p.under, c, params.k.y, p.i), p.coverage);
}

// `grid`'s share of a line at `at` along a way whose cells are `step`
// long, for lines `border` wide.
fn line(at: f32, step: f32, border: f32) -> f32 {
    let off = product(rem_euclid(divided(at, step), 1.0), step);
    let distance = min(off, step - off);
    return clamp(then(border / 2.0 - distance, 0.5), 0.0, 1.0);
}

// `grid`: spots[0] = anchor, cell; colours[0] = the lines' colour; k =
// border, opacity; flag = the cells rather than the lines.
@compute @workgroup_size(256)
fn grid(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = place(gid);
    if !p.covered {
        return;
    }
    let anchor = params.spots[0].xy;
    let cell = params.spots[0].zw;
    let border = params.k.x;
    var on = max(line(p.at.x - anchor.x, cell.x, border), line(p.at.y - anchor.y, cell.y, border));
    if params.flag != 0u {
        on = 1.0 - on;
    }
    let w = max(params.width, 1u);
    let c = params.colours[0].rgb;
    dst[p.i] = painted(composite(params.op, params.mode, w, p.under, c, on * params.k.y, p.i), p.coverage);
}

// `venetian_blinds`: k = completion, sine, cosine, width; k2.x = feather.
@compute @workgroup_size(256)
fn venetian_blinds(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = place(gid);
    if !p.covered {
        return;
    }
    let width = params.k.w;
    let feather = params.k2.x;
    let along = rem_euclid(product(p.at.x, params.k.y) + product(p.at.y, params.k.z), width);
    let edge = product(params.k.x, width);
    var shown: f32;
    if feather > 0.0 {
        shown = clamp(divided(along - edge, feather) + 0.5, 0.0, 1.0);
    } else {
        shown = select(0.0, 1.0, along >= edge);
    }
    dst[p.i] = painted(vec4(p.under.rgb, shown), p.coverage);
}

// `linear_wipe`: k = sine, cosine, the edge, the feather.
@compute @workgroup_size(256)
fn linear_wipe(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = place(gid);
    if !p.covered {
        return;
    }
    let along = product(p.at.x, params.k.x) - product(p.at.y, params.k.y);
    let beyond = then(along, -params.k.z);
    let shown = clamp(divided(beyond, max(params.k.w, 1.0)) + 0.5, 0.0, 1.0);
    dst[p.i] = painted(vec4(p.under.rgb, shown), p.coverage);
}

// `signed_distance`: how far `at` lies outside the polygon of `n` corners
// in `lines` from `extra`, negative inside, in the CPU's steps.
fn signed_distance(at: vec2<f32>) -> f32 {
    let n = params.n;
    var nearest = 3.4e38;
    var inside = false;
    for (var i = 0u; i < n; i++) {
        let j = (i + 1u) % n;
        let a = vec2(lines[params.extra + 2u * i], lines[params.extra + 2u * i + 1u]);
        let b = vec2(lines[params.extra + 2u * j], lines[params.extra + 2u * j + 1u]);
        let d = b - a;
        let along = product(at.x - a.x, d.x) + product(at.y - a.y, d.y);
        let t = clamp(divided(along, max(product(d.x, d.x) + product(d.y, d.y), 1e-12)), 0.0, 1.0);
        let q = vec2(then(a.x + product(t, d.x), -at.x), then(a.y + product(t, d.y), -at.y));
        nearest = min(nearest, product(q.x, q.x) + product(q.y, q.y));
        // Even-odd crossings of the ray to the right.
        if (a.y > at.y) != (b.y > at.y) && at.x < a.x + product(divided(at.y - a.y, b.y - a.y), d.x) {
            inside = !inside;
        }
    }
    let distance = sqrt(nearest);
    if inside {
        return -distance;
    }
    return distance;
}

// `iris_wipe`: n corners in `lines` from `extra`; k.x = the feather.
@compute @workgroup_size(256)
fn iris_wipe(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = place(gid);
    if !p.covered {
        return;
    }
    let outside = signed_distance(p.at);
    let feather = params.k.x;
    var shown: f32;
    if feather > 0.0 {
        shown = clamp(outside / feather + 1.0, 0.0, 1.0);
    } else {
        shown = clamp(outside + 0.5, 0.0, 1.0);
    }
    dst[p.i] = painted(vec4(p.under.rgb, shown), p.coverage);
}

// Whether the beam may draw at pixel `i`, (`px`, `py`): inside the
// layer's box by `lines` from `extra`, a column's then a row's (n = 1) or
// every pixel's (n = 2) worked out on the CPU, or everywhere (n = 0).
fn in_box(i: u32, px: u32, py: u32) -> bool {
    switch params.n {
        case 1u: {
            return lines[params.extra + px] > 0.0 && lines[params.extra + params.width + py] > 0.0;
        }
        case 2u: {
            return lines[params.extra + i] > 0.0;
        }
        default: {
            return true;
        }
    }
}

// `Pen::tapered` for `beam`, flat at its ends: spots[0] = where it starts
// from the whole pixel `spots[1].xy`, and from there to its end, in
// pixels of the grid; k = length², length, its half thickness at its
// start and at its end; k2.x = softness; colours[0], colours[1] = its
// inside and outside colours; flag = in place of the layer.
@compute @workgroup_size(256)
fn beam(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.y * params.row + gid.x;
    if i >= params.count {
        return;
    }
    var pixel = dst[i];
    if params.flag != 0u {
        pixel = vec4(0.0);
    }
    let w = max(params.width, 1u);
    let px = i % w;
    let py = i / w;
    let x = f32(px);
    let y = f32(py);
    let range = params.range;
    if x >= range.x && x < range.z && y >= range.y && y < range.w {
        let a = params.spots[0].xy;
        let ab = params.spots[0].zw;
        let length2 = params.k.x;
        let halves = params.k.zw;
        // From the beam's start, by way of the whole pixel near it, so the
        // floats stay small.
        let rel = vec2(x - params.spots[1].x + 0.5, y - params.spots[1].y + 0.5) - a;
        var t = 0.0;
        if length2 > 0.0 {
            t = clamp(dot(rel, ab) / length2, 0.0, 1.0);
        }
        let half = halves.x + (halves.y - halves.x) * t;
        let fade = max(half * params.k2.x * 2.0, 1.0);
        var distance = length(rel - ab * t);
        if length2 > 0.0 {
            let along = dot(rel, ab) / params.k.y;
            let past = max(along - params.k.y, -along);
            distance = max(distance, past + half);
        }
        let cover = clamp((half - distance) / fade + 0.5, 0.0, 1.0);
        if cover > 0.0 && in_box(i, px, py) {
            let edge = half + fade / 2.0;
            let across = clamp(distance / edge, 0.0, 1.0);
            let inside = params.colours[0];
            let colour = inside + (params.colours[1] - inside) * across;
            let s = colour.a * cover;
            if s > 0.0 {
                pixel = vec4(colour.rgb * s + pixel.rgb * (1.0 - s), s + pixel.a * (1.0 - s));
            }
        }
    }
    dst[i] = pixel;
}
