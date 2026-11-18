// Effects that read the pixels around each pixel (`blur.rs`): the sharpen
// and the edge finder. Each kernel ports the CPU function named beside it
// and writes a new grid from the one it reads; pixels are premultiplied
// RGBA floats.

struct Params {
    // How many pixels, how many invocations one row of workgroups covers,
    // and the grid's width and height.
    count: u32,
    row: u32,
    width: u32,
    height: u32,
    // find_edges: light lines on dark, rather than dark on light.
    light: u32,
    // sharpen: the amount; find_edges: the blend.
    k: f32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> src: array<vec4<f32>>;

fn index(gid: vec3<u32>) -> u32 {
    return gid.y * params.row + gid.x;
}

fn at(x: i32, y: i32) -> vec4<f32> {
    return src[u32(y) * params.width + u32(x)];
}

// `sharpen`: each pixel moved away from the mean of its four neighbours;
// past the grid's edges a pixel stands for its missing neighbours.
@compute @workgroup_size(256)
fn sharpen(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let x = i32(i % params.width);
    let y = i32(i / params.width);
    let w = i32(params.width);
    let h = i32(params.height);
    let me = src[i];
    var left = me;
    var right = me;
    var up = me;
    var down = me;
    if x > 0 {
        left = at(x - 1, y);
    }
    if x + 1 < w {
        right = at(x + 1, y);
    }
    if y > 0 {
        up = at(x, y - 1);
    }
    if y + 1 < h {
        down = at(x, y + 1);
    }
    let mean = (left + right + up + down) / 4.0;
    let p = clamp(me + (me - mean) * params.k, vec4(0.0), vec4(1.0));
    dst[i] = vec4(min(p.rgb, vec3(p.a)), p.a);
}

// The pixel `dx`, `dy` from (`x`, `y`), the edge pixels repeated beyond
// the grid.
fn near(x: i32, y: i32, dx: i32, dy: i32) -> vec3<f32> {
    let w = i32(params.width);
    let h = i32(params.height);
    return at(clamp(x + dx, 0, w - 1), clamp(y + dy, 0, h - 1)).rgb;
}

// `find-edges`: each colour channel's slope across its 3×3 pixels by the
// Sobel operator, a step from 0 to 1 giving 1, at most 1.
@compute @workgroup_size(256)
fn find_edges(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = index(gid);
    if i >= params.count {
        return;
    }
    let x = i32(i % params.width);
    let y = i32(i / params.width);
    let me = src[i];
    let a = me.a;
    let nw = near(x, y, -1, -1);
    let n = near(x, y, 0, -1);
    let ne = near(x, y, 1, -1);
    let w = near(x, y, -1, 0);
    let e = near(x, y, 1, 0);
    let sw = near(x, y, -1, 1);
    let s = near(x, y, 0, 1);
    let se = near(x, y, 1, 1);
    let across = (ne + 2.0 * e + se) - (nw + 2.0 * w + sw);
    let down = (sw + 2.0 * s + se) - (nw + 2.0 * n + ne);
    let edge = min(sqrt(across * across + down * down) / 4.0, vec3(1.0));
    var line = vec3(1.0) - edge;
    if params.light != 0u {
        line = edge;
    }
    let blend = params.k;
    // The colour channels stay premultiplied by the layer's alpha.
    dst[i] = vec4(line * a * (1.0 - blend) + me.rgb * blend, a);
}
