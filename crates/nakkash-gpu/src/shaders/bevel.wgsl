// The bevel (`light.rs`): how far inside the layer's outline each pixel
// lies, by an exact Euclidean distance (`columns`, then `rows`), and each
// pixel lit by the slope of the surface that rises over the bevel's
// thickness (`light`).
//
// The CPU finds every distance over the whole grid. Past the thickness the
// surface is flat, so here a squared distance is found within a reach
// just past it and held there: each pixel takes the nearest pixel outside
// within the reach along its column, then the nearest such column within
// the reach along its row, both whole numbers, as exact as the CPU's.

struct Params {
    // The grid's width and height, its pixels, and how many invocations
    // one row of workgroups covers.
    width: u32,
    height: u32,
    count: u32,
    row: u32,
    // How far a distance is looked for, in px, and its square, where
    // distances are held.
    reach: u32,
    cap: u32,
    // The bevel's thickness in px and its intensity.
    thickness: f32,
    intensity: f32,
    // Towards the light, and the light's straight colour.
    toward: vec4<f32>,
    light: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
// The layer's pixels.
@group(0) @binding(2) var<storage, read> src: array<vec4<f32>>;
// Squared distances, in `x`.
@group(0) @binding(3) var<storage, read> distances: array<vec4<f32>>;

fn index(gid: vec3<u32>) -> u32 {
    return gid.y * params.row + gid.x;
}

// Whether a pixel is inside the outline, where alpha crosses one half.
fn inside(i: u32) -> bool {
    return src[i].w >= 0.5;
}

// The squared distance to the nearest pixel outside along the column,
// within the reach.
@compute @workgroup_size(256)
fn columns(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    if !inside(i) {
        dst[i] = vec4(0.0);
        return;
    }
    let x = i % params.width;
    let y = i / params.width;
    var best = params.cap;
    for (var d = 1u; d <= params.reach; d++) {
        let up = d <= y && !inside((y - d) * params.width + x);
        let down = y + d < params.height && !inside((y + d) * params.width + x);
        if up || down {
            best = min(best, d * d);
            break;
        }
    }
    dst[i] = vec4(f32(best), 0.0, 0.0, 0.0);
}

// The squared distance to the nearest pixel outside, within the reach.
@compute @workgroup_size(256)
fn rows(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let x = i % params.width;
    let y = i / params.width;
    var best = u32(distances[i].x);
    let from_ = max(x, params.reach) - params.reach;
    let to = min(x + params.reach, params.width - 1u);
    for (var at = from_; at <= to; at++) {
        let d = max(at, x) - min(at, x);
        best = min(best, d * d + u32(distances[y * params.width + at].x));
    }
    dst[i] = vec4(f32(min(best, params.cap)), 0.0, 0.0, 0.0);
}

// What a neighbour of a pixel gives the slope: its rise (0 outside, up to
// 1 at the thickness), and its squared distance while it is still rising.
struct Rise {
    rise: f32,
    squared: f32,
    rising: bool,
}

fn rise_at(x: i32, y: i32) -> Rise {
    let cx = u32(clamp(x, 0, i32(params.width) - 1));
    let cy = u32(clamp(y, 0, i32(params.height) - 1));
    let i = cy * params.width + cx;
    if !inside(i) {
        return Rise(0.0, 0.0, false);
    }
    let squared = distances[i].x;
    let r = max(sqrt(squared) - 0.5, 0.0) / params.thickness;
    return Rise(min(r, 1.0), squared, r < 1.0);
}

// The surface's slope downhill one way, from the neighbours either side:
// the CPU's `-(high - low) / 2 · thickness`. Where both still rise it is
// half their depths' difference, worked out from their squared distances,
// whole numbers, so nothing large cancels.
fn downhill(low: Rise, high: Rise) -> f32 {
    if low.rising && high.rising {
        let a = sqrt(high.squared);
        let b = sqrt(low.squared);
        return -((high.squared - low.squared) / (a + b)) / 2.0;
    }
    return -(high.rise - low.rise) / 2.0 * params.thickness;
}

// `bevel`: every pixel coloured towards the light, or towards black, by
// how squarely its slope faces the light.
@compute @workgroup_size(256)
fn light(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let p = src[i];
    let a = p.w;
    if a <= 0.0 {
        dst[i] = p;
        return;
    }
    let x = i32(i % params.width);
    let y = i32(i / params.width);
    let down = vec2(
        downhill(rise_at(x - 1, y), rise_at(x + 1, y)),
        downhill(rise_at(x, y - 1), rise_at(x, y + 1)),
    );
    let facing = clamp(down.x * params.toward.x + down.y * params.toward.y, -1.0, 1.0);
    let k = facing * params.intensity;
    var c = p.rgb;
    if k >= 0.0 {
        c = c + (params.light.rgb * a - c) * k;
    } else {
        c = c * (1.0 + k);
    }
    dst[i] = vec4(c, a);
}
