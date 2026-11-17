//! Unit tests for `src/colour.rs`: which colour a clip's tags give.

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
