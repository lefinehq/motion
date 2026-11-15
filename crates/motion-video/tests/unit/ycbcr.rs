//! Unit tests for `src/ycbcr.rs`: which colour a clip's tags give, and the
//! conversion against the forward formulas `motion-encode` uses.

use super::*;

fn track(width: u32, height: u32, extensions: Vec<(FourCc, Vec<u8>)>) -> Track {
    Track {
        codec: *b"avc1",
        width,
        height,
        depth: 24,
        extensions,
        timescale: 30,
        samples: Vec::new(),
        shift: 0,
    }
}

fn nclx(matrix: u16, full: bool) -> (FourCc, Vec<u8>) {
    let mut body = b"nclx\0\x01\0\x01".to_vec();
    body.extend(matrix.to_be_bytes());
    body.push(if full { 0x80 } else { 0 });
    (*b"colr", body)
}

fn nclc(matrix: u16) -> (FourCc, Vec<u8>) {
    let mut body = b"nclc\0\x01\0\x01".to_vec();
    body.extend(matrix.to_be_bytes());
    (*b"colr", body)
}

fn signal(full_range: bool, matrix: Option<u8>) -> Option<Signal> {
    Some(Signal {
        full_range,
        description: matrix.map(|m| [1, 1, m]),
    })
}

#[test]
fn the_box_wins_then_the_stream_then_the_size() {
    let colour = |t: &Track, s| Colour::of(t, s, None).unwrap();
    let hd = |ext| track(1920, 1080, ext);
    let sd = |ext| track(720, 576, ext);
    // The box over the stream, for the matrix and the range.
    let c = colour(&hd(vec![nclx(1, true)]), signal(false, Some(6)));
    assert_eq!((c.matrix, c.full_range), (Matrix::Bt709, true));
    // QuickTime's box has no range: the stream's holds.
    let c = colour(&hd(vec![nclc(6)]), signal(true, Some(1)));
    assert_eq!((c.matrix, c.full_range), (Matrix::Bt601, true));
    // An unspecified matrix in the box leaves the stream's.
    let c = colour(&hd(vec![nclx(2, false)]), signal(false, Some(9)));
    assert_eq!(c.matrix, Matrix::Bt2020);
    // A profile box says nothing about the matrix.
    let c = colour(
        &sd(vec![(*b"colr", b"prof....".to_vec())]),
        signal(false, Some(1)),
    );
    assert_eq!(c.matrix, Matrix::Bt709);
    // Untagged: BT.709 above standard definition, BT.601 at it; limited.
    let c = colour(&hd(vec![]), None);
    assert_eq!((c.matrix, c.full_range), (Matrix::Bt709, false));
    assert_eq!(
        colour(&sd(vec![]), signal(false, Some(2))).matrix,
        Matrix::Bt601
    );
    assert_eq!(
        colour(&track(1280, 720, vec![]), None).matrix,
        Matrix::Bt709
    );
    assert_eq!(
        colour(&track(1080, 1920, vec![]), None).matrix,
        Matrix::Bt709
    );
    // Siting defaults to H.264's: left, between rows.
    assert_eq!(colour(&sd(vec![]), None).site, Site { x: 0, y: 1 });
    assert_eq!(
        Colour::of(&sd(vec![]), None, Some(1)).unwrap().site,
        Site { x: 1, y: 1 }
    );
}

#[test]
fn a_matrix_it_does_not_convert_is_named() {
    let e = Colour::of(&track(64, 32, vec![nclx(8, false)]), None, None).unwrap_err();
    assert!(e.message.contains("YCgCo"), "{}", e.message);
    let e = Colour::of(&track(64, 32, vec![]), signal(false, Some(0)), None).unwrap_err();
    assert!(e.message.contains("identity"), "{}", e.message);
}

/// `rgb` coded as Y′CbCr in `colour` at `bits`, by the forward formulas.
fn coded(rgb: [u8; 3], colour: Colour, bits: u32) -> [u32; 3] {
    let (kr, kb) = colour.matrix.weights();
    let [r, g, b] = rgb.map(|c| f64::from(c) / 255.0);
    let y = kr * r + (1.0 - kr - kb) * g + kb * b;
    let (cb, cr) = ((b - y) / (2.0 * (1.0 - kb)), (r - y) / (2.0 * (1.0 - kr)));
    let scale = f64::from(1u32 << (bits - 8));
    let top = f64::from((1u32 << bits) - 1);
    let [y, cb, cr] = match colour.full_range {
        false => [
            (16.0 + 219.0 * y) * scale,
            (128.0 + 224.0 * cb) * scale,
            (128.0 + 224.0 * cr) * scale,
        ],
        true => [
            y * top,
            f64::from(1u32 << (bits - 1)) + cb * top,
            f64::from(1u32 << (bits - 1)) + cr * top,
        ],
    };
    [y, cb, cr].map(|v| v.round().clamp(0.0, top) as u32)
}

/// A flat `width` by `height` picture of the coded colour, and its frame.
fn flat<T: Sample>(levels: [u32; 3], bits: u32, colour: Colour, sample: fn(u32) -> T) -> Frame {
    let (width, height) = (5, 3);
    let shift = T::WIDTH - bits;
    let luma = vec![sample(levels[0] << shift); width * height];
    let chroma: Vec<T> = (0..width.div_ceil(2) * height.div_ceil(2))
        .flat_map(|_| [sample(levels[1] << shift), sample(levels[2] << shift)])
        .collect();
    to_frame(
        Planes {
            width,
            height,
            luma: &luma,
            luma_stride: width,
            chroma: &chroma,
            chroma_stride: width.div_ceil(2) * 2,
            bits,
        },
        colour,
    )
}

#[test]
fn flat_colours_come_back_within_a_level_in_every_matrix_range_and_depth() {
    let colours = [
        [200, 60, 40],
        [40, 170, 80],
        [50, 80, 210],
        [230, 210, 60],
        [128, 128, 128],
        [16, 235, 128],
    ];
    let matrices = [
        Matrix::Bt709,
        Matrix::Bt601,
        Matrix::Fcc,
        Matrix::Smpte240,
        Matrix::Bt2020,
    ];
    for matrix in matrices {
        for full_range in [false, true] {
            let colour = Colour {
                matrix,
                full_range,
                site: Site { x: 0, y: 1 },
            };
            for rgb in colours {
                let eight = flat(coded(rgb, colour, 8), 8, colour, |v| v as u8);
                let ten = flat(coded(rgb, colour, 10), 10, colour, |v| v as u16);
                for (frame, bits, tolerance) in [(eight, 8, 2), (ten, 10, 1)] {
                    for pixel in frame.pixels.chunks_exact(4) {
                        let off = (0..3).map(|c| pixel[c].abs_diff(rgb[c])).max().unwrap();
                        assert!(
                            off <= tolerance && pixel[3] == 255,
                            "{matrix:?} full {full_range} {bits} bits: {rgb:?} came back {pixel:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn black_and_white_land_exactly_and_overshoot_clamps() {
    let colour = Colour {
        matrix: Matrix::Bt709,
        full_range: false,
        site: Site { x: 0, y: 1 },
    };
    let frame = |y| flat([y, 128, 128], 8, colour, |v| v as u8).pixels[..4].to_vec();
    assert_eq!(frame(16), [0, 0, 0, 255]);
    assert_eq!(frame(235), [255, 255, 255, 255]);
    assert_eq!(frame(0), [0, 0, 0, 255]);
    assert_eq!(frame(255), [255, 255, 255, 255]);
}

#[test]
fn chroma_between_samples_is_weighted_by_where_they_sit() {
    // Left sited: even pixels on a chroma sample, odd ones halfway.
    assert_eq!(
        taps(5, 3, 0),
        [(0, 1, 0), (0, 1, 2), (1, 2, 0), (1, 2, 2), (2, 2, 0)]
    );
    // Centred: a quarter of the way either side; the edges clamp.
    assert_eq!(taps(4, 2, 1), [(0, 0, 3), (0, 1, 1), (0, 1, 3), (1, 1, 1)]);
    // Rows sited on the lower luma row of each pair.
    assert_eq!(taps(4, 2, 2), [(0, 0, 2), (0, 1, 0), (0, 1, 2), (1, 1, 0)]);
}

#[test]
fn a_chroma_step_blends_over_the_pixels_between_samples() {
    // Two chroma columns, the left neutral, the right full red-difference.
    let colour = Colour {
        matrix: Matrix::Bt709,
        full_range: false,
        site: Site { x: 0, y: 1 },
    };
    let luma = vec![126u8; 4 * 2];
    let chroma = [128u8, 128, 128, 240];
    let frame = to_frame(
        Planes {
            width: 4,
            height: 2,
            luma: &luma,
            luma_stride: 4,
            chroma: &chroma,
            chroma_stride: 4,
            bits: 8,
        },
        colour,
    );
    let red: Vec<u8> = frame.pixels.chunks_exact(4).take(4).map(|p| p[0]).collect();
    // Grey, halfway, then the right sample's red from pixel 2 on.
    assert_eq!(red[0], 128);
    assert!(red[1] > red[0] && red[1] < red[2], "{red:?}");
    assert_eq!(red[2], red[3]);
}
