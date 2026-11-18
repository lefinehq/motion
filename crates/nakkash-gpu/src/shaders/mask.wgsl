// Layer masks (`mask.rs`): a mask's coverage grown or shrunk, and combined
// with the masks before it. Coverage is held in every channel of a pixel,
// as the CPU feathers it; the feather itself is the Gaussian blur's box
// passes.

struct Params {
    // How many pixels, how many invocations one row of workgroups covers,
    // and the grid's size.
    count: u32,
    row: u32,
    width: u32,
    height: u32,
    // `spread`: the window's radius in px, whether it takes the largest
    // value (else the smallest), and whether it runs down the columns
    // (else along the rows).
    radius: u32,
    grow: u32,
    columns: u32,
    // `combine`: whether this is the first mask, the coverage it starts
    // from, whether the mask is inverted, its opacity and its mode.
    first: u32,
    start: f32,
    inverted: u32,
    opacity: f32,
    mode: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> src: array<vec4<f32>>;

fn index(gid: vec3<u32>) -> u32 {
    return gid.y * params.row + gid.x;
}

// `spread`/`window`: the largest (or smallest) coverage within `radius` px
// one way; past the grid's edge the edge px repeats, so the window is cut
// to the grid.
@compute @workgroup_size(256)
fn spread(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let x = i % params.width;
    let y = i / params.width;
    var at = x;
    var n = params.width;
    var step = 1u;
    if params.columns != 0u {
        at = y;
        n = params.height;
        step = params.width;
    }
    let first = i - at * step;
    let lo = select(0u, at - params.radius, at >= params.radius);
    let hi = min(at + params.radius, n - 1u);
    var best = src[first + lo * step].w;
    for (var j = lo + 1u; j <= hi; j++) {
        let v = src[first + j * step].w;
        if params.grow != 0u {
            best = max(best, v);
        } else {
            best = min(best, v);
        }
    }
    dst[i] = vec4(best);
}

// `combine`: the coverage so far with one more mask's, by the mask's mode.
@compute @workgroup_size(256)
fn combine(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    var a = params.start;
    if params.first == 0u {
        a = dst[i].w;
    }
    var m = clamp(src[i].w, 0.0, 1.0);
    if params.inverted != 0u {
        m = 1.0 - m;
    }
    m = m * params.opacity;
    switch params.mode {
        case 0u: { a = a + m - a * m; }
        case 1u: { a = a * (1.0 - m); }
        case 2u: { a = a * m; }
        case 3u: { a = max(a, m); }
        case 4u: { a = min(a, m); }
        case 5u: { a = abs(a - m); }
        default: {}
    }
    dst[i] = vec4(a);
}
