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
fn without_an_edit_list_the_first_frame_shows_at_0() {
    // Its composition offsets start at two frames, as an encoder that
    // reorders frames writes them, and nothing shifts them back.
    let t = track("h264-count-noedit.mp4");
    assert!(t.samples.iter().all(|s| s.show >= 2002));
    assert_eq!(t.shift, -2002);
    let mut shown: Vec<i64> = t.samples.iter().map(|s| s.show + t.shift).collect();
    shown.sort_unstable();
    assert_eq!(shown, (0..20).map(|i| i * 1001).collect::<Vec<_>>());
    // With an edit list, the list says the same.
    assert_eq!(track("h264-count.mp4").shift, -2002);
}

#[test]
fn decoding_a_leading_picture_starts_from_the_group_before() {
    let t = track("h264-opengop.mp4");
    let samples = &t.samples;
    // A sync sample after the first, with pictures decoded after it that
    // show before it.
    let sync = (1..samples.len()).find(|&i| samples[i].sync).unwrap();
    let leading = (sync + 1..samples.len())
        .find(|&i| samples[i].show < samples[sync].show)
        .expect("a leading picture");
    let after = (leading + 1..samples.len())
        .find(|&i| samples[i].show > samples[sync].show)
        .expect("a picture after the sync sample");
    assert_eq!(t.start_for(sync), sync);
    // Both the leading picture and a later one decoded after it go back.
    assert_eq!(t.start_for(leading), t.sync_before(sync - 1));
    assert_eq!(t.start_for(after), t.sync_before(sync - 1));
    // In a closed group, every picture starts from its own sync sample.
    let t = track("h264-count.mp4");
    assert!((0..20).all(|i| t.start_for(i) == t.sync_before(i)));
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
