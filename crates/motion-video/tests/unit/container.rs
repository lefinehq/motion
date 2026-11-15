//! Unit tests for `src/container.rs`, on clips made for them.

use super::*;
use std::fs::File;

fn track(name: &str) -> Track {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    read(&mut File::open(path).unwrap()).unwrap()
}

#[test]
fn a_quicktime_track_gives_its_codec_size_and_samples() {
    let t = track("argb.mov");
    assert_eq!(
        (t.codec, t.width, t.height, t.depth),
        (*b"rle ", 32, 16, 32)
    );
    assert_eq!(t.samples.len(), 10);
    assert!((t.seconds() - 1.0).abs() < 1e-9, "{}", t.seconds());
    // Frames follow each other in the file and in time.
    for pair in t.samples.windows(2) {
        assert_eq!(pair[1].decode - pair[0].decode, i64::from(pair[0].duration));
        assert!(pair[1].offset >= pair[0].offset + u64::from(pair[0].size));
    }
    let sync: Vec<usize> = (0..10).filter(|&i| t.samples[i].sync).collect();
    assert_eq!(sync, [0, 4, 8]);
    assert_eq!(
        (t.sync_before(3), t.sync_before(4), t.sync_before(9)),
        (0, 4, 8)
    );
}

#[test]
fn an_mp4_track_shows_its_frames_in_order_through_its_edit_list() {
    let t = track("bframes.mp4");
    assert_eq!(&t.codec, b"avc1");
    assert!(t.extensions.iter().any(|(kind, _)| kind == b"avcC"));
    // B-frames decode out of the order they show in.
    assert!(t.samples.windows(2).any(|p| p[1].show < p[0].show));
    // On the clip's own time the shown frames start at 0 and step a frame.
    let mut shown: Vec<i64> = t.samples.iter().map(|s| s.show + t.shift).collect();
    shown.sort_unstable();
    let frame = i64::from(t.timescale) / 10;
    assert_eq!(shown, (0..10).map(|i| i * frame).collect::<Vec<_>>());
    assert!((t.seconds() - 1.0).abs() < 1e-9, "{}", t.seconds());
    let sync: Vec<usize> = (0..10).filter(|&i| t.samples[i].sync).collect();
    assert_eq!(sync, [0, 5]);
}

#[test]
fn a_file_that_is_not_a_clip_says_so() {
    let mut bytes = std::io::Cursor::new(b"\x00\x00\x00\x08free".to_vec());
    let e = read(&mut bytes).unwrap_err();
    assert!(e.message.contains("no movie box"), "{}", e.message);
    let mut bytes = std::io::Cursor::new(b"\x00\x00\x01\x00moov".to_vec());
    let e = read(&mut bytes).unwrap_err();
    assert!(e.message.contains("runs past the end"), "{}", e.message);
}
