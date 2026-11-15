//! Unit tests for `src/lib.rs`: frames against frames decoded independently,
//! times, and seeking.

use super::*;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
}

fn clip(name: &str) -> Clip {
    Clip::open(&fixture(name)).unwrap()
}

/// Frame `i` of the reference frames decoded for `name`.
fn reference(name: &str, i: usize) -> Vec<u8> {
    let all = std::fs::read(fixture(&format!("{name}.rgba"))).unwrap();
    let size = 32 * 16 * 4;
    all[i * size..(i + 1) * size].to_vec()
}

/// The frame showing in the middle of frame `i` of a 10 fps clip.
fn frame(clip: &Clip, i: usize) -> Arc<Frame> {
    clip.frame_at((i as f64 + 0.5) / 10.0).unwrap().unwrap()
}

#[test]
fn quicktime_animation_decodes_exactly_with_and_without_alpha() {
    for name in ["argb", "rgb"] {
        let c = clip(&format!("{name}.mov"));
        assert_eq!(c.size(), (32, 16));
        for i in 0..10 {
            assert_eq!(frame(&c, i).pixels, reference(name, i), "{name} frame {i}");
        }
    }
    // The ARGB clip keeps its half-transparent left side.
    let c = clip("argb.mov");
    assert_eq!(frame(&c, 0).pixels[3], 128);
}

/// The reference frames of `photo.mov` are its JPEGs decoded by libjpeg.
/// Decoders upsample chroma their own way, so libjpeg, another system
/// decoder and ffmpeg differ from each other by 5 to 7 on average on this
/// small, sharp picture; ours stays within a few levels of libjpeg.
#[test]
fn photo_jpeg_decodes_as_libjpeg_does_within_rounding() {
    let c = clip("photo.mov");
    for i in 0..10 {
        let got = &frame(&c, i).pixels;
        let want = reference("photo", i);
        let worst = got
            .iter()
            .zip(&want)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(worst <= 4, "frame {i}: off by {worst}");
        let mean = got
            .iter()
            .zip(&want)
            .map(|(a, b)| f64::from(a.abs_diff(*b)))
            .sum::<f64>()
            / got.len() as f64;
        assert!(mean < 0.25, "frame {i}: mean {mean}");
    }
}

#[test]
fn seeking_anywhere_gives_the_frames_of_a_straight_pass() {
    let straight: Vec<Vec<u8>> = {
        let c = clip("argb.mov");
        (0..10).map(|i| frame(&c, i).pixels.clone()).collect()
    };
    let c = clip("argb.mov");
    for i in [9, 2, 5, 4, 0, 7, 7, 3, 8, 1, 6] {
        assert_eq!(frame(&c, i).pixels, straight[i], "frame {i}");
    }
}

#[test]
fn nothing_shows_before_the_clip_or_from_its_end() {
    let c = clip("rgb.mov");
    assert!((c.seconds() - 1.0).abs() < 1e-9);
    assert!(c.frame_at(-0.01).unwrap().is_none());
    assert!(c.frame_at(1.0).unwrap().is_none());
    assert!(c.frame_at(0.0).unwrap().is_some());
    assert_eq!(
        c.frame_at(0.99).unwrap().unwrap().pixels,
        reference("rgb", 9)
    );
}

#[test]
fn a_codec_without_a_decoder_is_named() {
    let e = Clip::open(&fixture("bframes.mp4")).err().unwrap();
    assert!(e.message.contains("H.264"), "{}", e.message);
    assert!(e.message.contains("bframes.mp4"), "{}", e.message);
    assert!(!decodes(*b"avc1") && decodes(*b"rle ") && decodes(*b"jpeg"));
    assert_eq!(codec_name(*b"zzzz"), "`zzzz`");
}
