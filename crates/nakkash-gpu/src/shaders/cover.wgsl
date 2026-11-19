// `bezier-warp`'s pieces laid on the grid (`distort::cover`): every pixel
// whose centre a triangle of the mesh holds is marked with the triangle's
// number, one past its place in the order the CPU covers them, keeping
// the largest: where the patch folds over itself, the CPU's last triangle
// shows, and so does the largest number. Each invocation covers one band
// of rows of one triangle.
//
// The mesh buffer (`WarpMesh` in `distort.rs`): the patch's twelve points,
// the corners of its pieces on the grid, then a number for every pixel,
// all as 32-bit words.

struct Params {
    // How many triangles, and how many pieces along each side.
    count: u32,
    pieces: u32,
    width: u32,
    height: u32,
    // Where the corners and the pixels' numbers start in `mesh`.
    corners: u32,
    shows: u32,
    // How many rows a band holds.
    band: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> mesh: array<atomic<u32>>;

// The corner in column `i` of row `j`, on the grid.
fn corner(i: u32, j: u32) -> vec2<f32> {
    let k = params.corners + 2u * (j * (params.pieces + 1u) + i);
    return vec2(bitcast<f32>(atomicLoad(&mesh[k])), bitcast<f32>(atomicLoad(&mesh[k + 1u])));
}

fn cross2(a: vec2<f32>, b: vec2<f32>) -> f32 {
    return a.x * b.y - a.y * b.x;
}

@compute @workgroup_size(256)
fn warp_cover(@builtin(global_invocation_id) gid: vec3<u32>) {
    let t = gid.x;
    if t >= params.count {
        return;
    }
    let quad = t / 2u;
    let i = quad % params.pieces;
    let j = quad / params.pieces;
    let a = corner(i, j);
    var b = corner(i + 1u, j);
    var c = corner(i + 1u, j + 1u);
    if t % 2u == 1u {
        b = c;
        c = corner(i, j + 1u);
    }
    let area = cross2(b - a, c - a);
    if abs(area) < 1e-12 {
        return;
    }
    let lo = min(min(a, b), c);
    let hi = max(max(a, b), c);
    let x0 = u32(max(ceil(lo.x - 0.5), 0.0));
    let y0 = u32(max(ceil(lo.y - 0.5), 0.0));
    let x1 = u32(clamp(floor(hi.x - 0.5) + 1.0, 0.0, f32(params.width)));
    let y1 = u32(clamp(floor(hi.y - 0.5) + 1.0, 0.0, f32(params.height)));
    let first = max(y0, gid.y * params.band);
    let last = min(y1, gid.y * params.band + params.band);
    // An overlap of neighbouring triangles, wider than the CPU's for the
    // rounding of 32-bit corners, so they leave no gap: what one holds
    // past its edge its neighbour shows the same.
    let overlap = -1e-5;
    for (var y = first; y < last; y++) {
        for (var x = x0; x < x1; x++) {
            let p = vec2(f32(x) + 0.5, f32(y) + 0.5);
            let wa = cross2(b - p, c - p) / area;
            let wb = cross2(c - p, a - p) / area;
            let wc = 1.0 - wa - wb;
            if wa >= overlap && wb >= overlap && wc >= overlap {
                atomicMax(&mesh[params.shows + y * params.width + x], t + 1u);
            }
        }
    }
}
