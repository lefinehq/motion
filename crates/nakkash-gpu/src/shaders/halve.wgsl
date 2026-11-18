// A projected layer's buffer and its halved copies (`projected.rs`,
// `Texture::halved`), one after another in one buffer of 32-bit words:
// each copy half the size of the one before, rounded up, each px the mean
// of the four it covers, clear beyond the edges. A buffer the rasteriser
// drew keeps its 8-bit px, one word each, their means rounded as the CPU
// rounds them; a float buffer takes four words a px.

struct Params {
    count: u32,
    row: u32,
    // Whether the px are 8-bit (1) or floats (0).
    bytes: u32,
    // Where the copy read starts, in px, and its size.
    source: u32,
    source_width: u32,
    source_height: u32,
    // Where the copy written starts, and its width.
    written: u32,
    written_width: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> chain: array<u32>;

// The four 8-bit channels of a px spread over two words, 16 bits a lane:
// red and green in the first, blue and alpha in the second.
fn spread(p: u32) -> vec2<u32> {
    return vec2((p & 0xffu) | ((p & 0xff00u) << 8u), ((p >> 16u) & 0xffu) | ((p >> 8u) & 0xff0000u));
}

// The px at `x`, `y` of the copy read, as 8-bit, 0 beyond its edges.
fn byte_px(x: u32, y: u32) -> u32 {
    if x >= params.source_width || y >= params.source_height {
        return 0u;
    }
    return chain[params.source + y * params.source_width + x];
}

// The px at `x`, `y` of the copy read, as floats, clear beyond its edges.
fn float_px(x: u32, y: u32) -> vec4<f32> {
    if x >= params.source_width || y >= params.source_height {
        return vec4(0.0);
    }
    let j = 4u * (params.source + y * params.source_width + x);
    return vec4(bitcast<f32>(chain[j]), bitcast<f32>(chain[j + 1u]), bitcast<f32>(chain[j + 2u]), bitcast<f32>(chain[j + 3u]));
}

@compute @workgroup_size(256)
fn halve(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.y * params.row + gid.x;
    if i >= params.count {
        return;
    }
    let x = 2u * (i % params.written_width);
    let y = 2u * (i / params.written_width);
    let at = params.written + i;
    if params.bytes != 0u {
        // `<[u8; 4] as Texel>::mean`: the sums of four in 16-bit lanes,
        // each plus 2 and divided by 4.
        let sum = spread(byte_px(x, y)) + spread(byte_px(x + 1u, y)) + spread(byte_px(x, y + 1u)) + spread(byte_px(x + 1u, y + 1u));
        let r = ((sum.x & 0xffffu) + 2u) / 4u;
        let g = ((sum.x >> 16u) + 2u) / 4u;
        let b = ((sum.y & 0xffffu) + 2u) / 4u;
        let a = ((sum.y >> 16u) + 2u) / 4u;
        chain[at] = r | (g << 8u) | (b << 16u) | (a << 24u);
        return;
    }
    // `<Pixel as Texel>::mean`.
    let mean = (float_px(x, y) + float_px(x + 1u, y) + float_px(x, y + 1u) + float_px(x + 1u, y + 1u)) * 0.25;
    let j = 4u * at;
    chain[j] = bitcast<u32>(mean.x);
    chain[j + 1u] = bitcast<u32>(mean.y);
    chain[j + 2u] = bitcast<u32>(mean.z);
    chain[j + 3u] = bitcast<u32>(mean.w);
}
