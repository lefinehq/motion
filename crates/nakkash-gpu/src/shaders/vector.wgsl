// The vector blur (`vector.rs`): the slope of one level of a map at every
// pixel (`slopes`), the steepest of them over the grid (`steepest`, then
// `most`), and each pixel the mean of the layer along a streak the slope
// gives it (`smear`). The map's level and the slope are softened between
// them by the blur family's Gaussians.

struct Params {
    // The grid's width and height, its pixels, and how many invocations
    // one row of workgroups covers.
    width: u32,
    height: u32,
    count: u32,
    row: u32,
    // How each streak follows the slope (`Streak`, in its order) and the
    // level of the map (red, green, blue, alpha, luminance, lightness, hue,
    // saturation).
    streak: u32,
    level: u32,
    // How many pixels the map holds, and how many partial maxima there are.
    map: u32,
    partials: u32,
    // The streaks' length at the steepest, in px of the grid, and the
    // sine and cosine of the turn every vector takes, as the CPU works
    // them out.
    amount: f32,
    sin: f32,
    cos: f32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> field: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> most: array<vec4<f32>>;

const STREAK_CONSTANT: u32 = 1u;
const STREAK_ACROSS: u32 = 2u;
const STREAK_BEHIND: u32 = 3u;
const STREAK_FADING: u32 = 4u;

// The most samples a streak takes (`MOST_SAMPLES`).
const MOST_SAMPLES: u32 = 96u;

fn index(gid: vec3<u32>) -> u32 {
    return gid.y * params.row + gid.x;
}

// `Level::of`: the level of the map's pixel `p`.
fn level_of(p: vec4<f32>) -> f32 {
    let a = p.w;
    var c = vec3(0.0);
    if a > 0.0 {
        c = p.rgb / a;
    }
    let r = c.x;
    let g = c.y;
    let b = c.z;
    let hi = max(max(r, g), b);
    let lo = min(min(r, g), b);
    switch params.level {
        case 0u: {
            return r;
        }
        case 1u: {
            return g;
        }
        case 2u: {
            return b;
        }
        case 3u: {
            return a;
        }
        case 4u: {
            return r * 0.299 + g * 0.587 + b * 0.114;
        }
        case 5u: {
            return (hi + lo) / 2.0;
        }
        case 6u: {
            let d = hi - lo;
            if d <= 0.0 {
                return 0.0;
            } else if hi == r {
                // `rem_euclid(6)` of a value of -1 to 1.
                var h = (g - b) / d;
                if h < 0.0 {
                    h += 6.0;
                }
                return h / 6.0;
            } else if hi == g {
                return ((b - r) / d + 2.0) / 6.0;
            }
            return ((r - g) / d + 4.0) / 6.0;
        }
        default: {
            let l = (hi + lo) / 2.0;
            let d = hi - lo;
            if d <= 0.0 {
                return 0.0;
            }
            return d / max(1.0 - abs(2.0 * l - 1.0), 1e-6);
        }
    }
}

fn level_at(x: u32, y: u32) -> f32 {
    let i = y * params.width + x;
    if i >= params.map {
        return level_of(vec4(0.0));
    }
    return level_of(src[i]);
}

// `slopes`: the level's slope at every pixel, x in red and y in green,
// from its neighbours either side (the pixel itself at an edge).
@compute @workgroup_size(256)
fn slopes(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let x = i % params.width;
    let y = i / params.width;
    let left = max(x, 1u) - 1u;
    let right = min(x + 1u, params.width - 1u);
    let up = max(y, 1u) - 1u;
    let down = min(y + 1u, params.height - 1u);
    let dx = (level_at(right, y) - level_at(left, y)) / f32(max(right - left, 1u));
    let dy = (level_at(x, down) - level_at(x, up)) / f32(max(down - up, 1u));
    dst[i] = vec4(dx, dy, 0.0, 0.0);
}

fn slope_of(v: vec4<f32>) -> f32 {
    return sqrt(v.x * v.x + v.y * v.y);
}

// What each workgroup of `steepest` and `most` holds of its maxima.
var<workgroup> held: array<f32, 256>;

// The largest of `held`, into `held[0]`.
fn reduce(local: u32) {
    for (var stride = 128u; stride > 0u; stride >>= 1u) {
        workgroupBarrier();
        if local < stride {
            held[local] = max(held[local], held[local + stride]);
        }
    }
    workgroupBarrier();
}

// The steepest slope of each 4,096 pixels.
@compute @workgroup_size(256)
fn steepest(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) local: u32,
) {
    let start = (group.x * 256u + local) * 16u;
    var m = 0.0;
    for (var k = 0u; k < 16u; k++) {
        if start + k < params.count {
            m = max(m, slope_of(field[start + k]));
        }
    }
    held[local] = m;
    reduce(local);
    if local == 0u {
        dst[group.x] = vec4(held[0], 0.0, 0.0, 0.0);
    }
}

// The steepest slope of all, from the partial maxima.
@compute @workgroup_size(256)
fn most_of(@builtin(local_invocation_index) local: u32) {
    var m = 0.0;
    for (var k = local; k < params.partials; k += 256u) {
        m = max(m, most[k].x);
    }
    held[local] = m;
    reduce(local);
    if local == 0u {
        dst[0] = vec4(held[0], 0.0, 0.0, 0.0);
    }
}

fn pixel_at(x: i32, y: i32) -> vec4<f32> {
    if x < 0 || y < 0 || x >= i32(params.width) || y >= i32(params.height) {
        return vec4(0.0);
    }
    return src[u32(y) * params.width + u32(x)];
}

// `Pixels::sample` at a pixel's centre moved by `o`: the px it starts in
// and how far into it are the pixel's own and `o`'s, so they are exact.
fn sample_from(x: u32, y: u32, o: vec2<f32>) -> vec4<f32> {
    let w = floor(o);
    let f = o - w;
    let x0 = i32(x) + i32(w.x);
    let y0 = i32(y) + i32(w.y);
    let a = pixel_at(x0, y0);
    let b = pixel_at(x0 + 1, y0);
    let c = pixel_at(x0, y0 + 1);
    let d = pixel_at(x0 + 1, y0 + 1);
    let top = a + (b - a) * f.x;
    let bottom = c + (d - c) * f.x;
    return top + (bottom - top) * f.y;
}

// `smear`: every pixel the mean of the layer along its streak.
@compute @workgroup_size(256)
fn smear(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let own = src[i];
    let steep = most[0].x;
    if steep <= 1e-9 && params.streak != STREAK_CONSTANT {
        dst[i] = own;
        return;
    }
    let steepest = max(steep, 1e-9);
    let g = field[i];
    let slope = slope_of(g);
    if slope <= 1e-9 {
        dst[i] = own;
        return;
    }
    var dx = g.x / slope;
    var dy = g.y / slope;
    let turned_x = dx * params.cos - dy * params.sin;
    let turned_y = dx * params.sin + dy * params.cos;
    dx = turned_x;
    dy = turned_y;
    if params.streak == STREAK_ACROSS {
        let t = dx;
        dx = -dy;
        dy = t;
    }
    var length = params.amount * min(slope / steepest, 1.0);
    if params.streak == STREAK_CONSTANT {
        length = params.amount;
    }
    if abs(length) < 0.05 {
        dst[i] = own;
        return;
    }
    var from_ = -length / 2.0;
    var to = length / 2.0;
    if params.streak == STREAK_BEHIND || params.streak == STREAK_FADING {
        from_ = -length;
        to = 0.0;
    }
    let samples = clamp(u32(min(ceil(abs(to - from_)), 1000.0)) + 1u, 2u, MOST_SAMPLES);
    let x = i % params.width;
    let y = i / params.width;
    var sum = vec4(0.0);
    var weight = 0.0;
    for (var k = 0u; k < samples; k++) {
        let s = f32(k) / f32(samples - 1u);
        let along = from_ + (to - from_) * s;
        var w = 1.0;
        if params.streak == STREAK_FADING {
            // Full at the pixel (s = 1), nothing at the far end.
            w = s;
        }
        sum += sample_from(x, y, vec2(dx * along, dy * along)) * w;
        weight += w;
    }
    dst[i] = sum / weight;
}
