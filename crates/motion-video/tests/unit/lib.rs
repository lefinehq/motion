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
    // The B-frame clip with its codec renamed to MPEG-4 Part 2.
    let mut bytes = std::fs::read(fixture("bframes.mp4")).unwrap();
    let stsd = bytes.windows(4).position(|w| w == b"stsd").unwrap();
    let at = stsd + bytes[stsd..].windows(4).position(|w| w == b"avc1").unwrap();
    bytes[at..at + 4].copy_from_slice(b"mp4v");
    let path = std::env::temp_dir().join(format!("motion-video-mp4v-{}.mp4", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let e = Clip::open(&path).err().unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(e.message.contains("MPEG-4 Part 2"), "{}", e.message);
    assert!(e.message.contains("motion-video-mp4v"), "{}", e.message);
    assert!(decodes(*b"rle ") && decodes(*b"jpeg") && !decodes(*b"mp4v"));
    assert_eq!(codec_name(*b"zzzz"), "`zzzz`");
}

#[cfg(not(target_os = "macos"))]
#[test]
fn h264_says_it_needs_the_system_decoder_elsewhere() {
    let e = Clip::open(&fixture("h264-count.mp4")).err().unwrap();
    assert!(e.message.contains("H.264"), "{}", e.message);
    assert!(e.message.contains("operating system"), "{}", e.message);
    assert!(!decodes(*b"avc1"));
}

/// Tests of H.264 through the system's decoder. The clips were made for
/// them with ffmpeg's libx264 from generated pictures (no third-party
/// footage), by these commands, `quad.ppm` and `quad-hd.ppm` being four
/// flat quadrants of (200, 60, 40), (40, 170, 80), (50, 80, 210) and
/// (230, 210, 60), 64 by 32 and 1280 by 720:
///
/// ```text
/// X="-c:v libx264 -preset medium -threads 1 -bf 2 -g 8 -keyint_min 8 -sc_threshold 0 -b_strategy 0"
/// COUNT="color=c=black:s=64x32:r=24000/1001:d=0.8342,format=yuv420p,geq=lum='16+10*N':cb=128:cr=128"
/// ffmpeg -f lavfi -i "$COUNT" -frames:v 20 $X -qp 4 h264-count.mp4
/// ffmpeg -f lavfi -i "$COUNT" -frames:v 20 $X -qp 4 -use_editlist 0 h264-count-noedit.mp4
/// ffmpeg -f lavfi -i "$COUNT" -frames:v 20 $X -qp 4 -bf 3 -x264-params open_gop=1 h264-opengop.mp4
/// LONG="color=c=black:s=64x32:r=24000/1001:d=1.6684,format=yuv420p,geq=lum='16+5*N':cb=128:cr=128"
/// ffmpeg -f lavfi -i "$LONG" -frames:v 40 -c:v libx264 -preset medium -threads 1 -bf 2 \
///   -b_strategy 0 -g 40 -sc_threshold 0 -x264-params b-pyramid=none -qp 4 h264-long.mp4
/// Q="-c:v libx264 -preset medium -threads 1 -qp 2 -frames:v 2"
/// T="-color_primaries bt709 -color_trc bt709 -colorspace bt709"
/// U="setparams=range=unknown:color_primaries=unknown:color_trc=unknown:colorspace=unknown"
/// ffmpeg -i quad.ppm -vf "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p" $Q $T \
///   -color_range tv -movflags +write_colr h264-bt709.mp4
/// ffmpeg -i quad.ppm -vf "scale=out_color_matrix=bt601:out_range=tv,format=yuv420p" $Q \
///   -color_primaries smpte170m -color_trc bt709 -colorspace smpte170m -color_range tv \
///   -bsf:v h264_metadata=chroma_sample_loc_type=1 h264-bt601.mp4
/// ffmpeg -i quad.ppm -vf "scale=out_color_matrix=bt709:out_range=pc,format=yuv420p" $Q $T \
///   -color_range pc h264-full.mp4
/// ffmpeg -i quad.ppm -vf "scale=out_color_matrix=bt601:out_range=tv,format=yuv420p,$U" $Q \
///   h264-untagged-sd.mp4
/// ffmpeg -i quad-hd.ppm -vf "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p,$U" $Q \
///   h264-untagged-hd.mp4
/// ffmpeg -i quad.ppm -vf "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p" $Q $T \
///   -color_range tv -movflags +write_colr -bsf:v h264_metadata=matrix_coefficients=6 \
///   h264-box-wins.mp4
/// ```
#[cfg(target_os = "macos")]
mod h264 {
    use super::*;

    /// Frames a second of the counting clips.
    const FPS: f64 = 24000.0 / 1001.0;
    /// Seconds a frame of the counting clips lasts.
    const FRAME: f64 = 1001.0 / 24000.0;

    /// The grey frame `n` of a counting clip shows: luma 16 + `step` `n`.
    fn level(n: usize, step: usize) -> u8 {
        ((step * n) as f64 * 255.0 / 219.0).round() as u8
    }

    /// The frame a picture of a counting clip of `frames` frames is, by its
    /// grey.
    fn number(frame: &Frame, frames: usize, step: usize) -> usize {
        let grey = frame.pixels[4 * (frame.width as usize * 8 + 16)];
        let n = (0..frames)
            .min_by_key(|&n| level(n, step).abs_diff(grey))
            .unwrap();
        assert!(level(n, step).abs_diff(grey) <= 2, "grey {grey}");
        n
    }

    fn at(clip: &Clip, seconds: f64) -> Option<usize> {
        clip.frame_at(seconds).unwrap().map(|f| number(&f, 20, 10))
    }

    #[test]
    fn each_frame_shows_from_its_time_to_the_next_with_and_without_an_edit_list() {
        for name in ["h264-count.mp4", "h264-count-noedit.mp4"] {
            let c = clip(name);
            assert_eq!(c.size(), (64, 32));
            assert!(
                (c.seconds() - 20.0 * FRAME).abs() < 1e-9,
                "{name}: {}",
                c.seconds()
            );
            for n in 0..20 {
                // As a renderer at the clip's rate asks for it: frame 3 is
                // 3 / FPS seconds in, which falls a hair short of 3003
                // ticks, as do frames 6, 9, 12, 13, 17 and 18.
                let start = n as f64 / FPS;
                assert_eq!(at(&c, start), Some(n), "{name}: the start of frame {n}");
                assert_eq!(at(&c, start + FRAME / 2.0), Some(n), "{name}: frame {n}");
                if n > 0 {
                    assert_eq!(at(&c, start - 1e-6), Some(n - 1), "{name}: before {n}");
                }
            }
            assert_eq!(at(&c, 20.0 * FRAME), None, "{name}: the end");
            assert_eq!(at(&c, -1e-3), None, "{name}: before the start");
        }
    }

    #[test]
    fn seeking_anywhere_gives_the_frames_of_a_straight_pass() {
        for name in ["h264-count.mp4", "h264-opengop.mp4"] {
            let c = clip(name);
            let middle = |n: usize| (n as f64 + 0.5) * FRAME;
            for n in 0..20 {
                assert_eq!(at(&c, middle(n)), Some(n), "{name}: pass, frame {n}");
            }
            let c = clip(name);
            for n in [19, 3, 12, 0, 7, 7, 15, 1, 8, 18, 5, 6, 4] {
                assert_eq!(at(&c, middle(n)), Some(n), "{name}: seek, frame {n}");
            }
        }
    }

    #[test]
    fn a_leading_picture_of_an_open_group_decodes_from_the_group_before() {
        // Each frame, from a clip opened for it alone: the container's
        // tests show the clip has leading pictures.
        for n in 0..20 {
            let c = clip("h264-opengop.mp4");
            assert_eq!(at(&c, (n as f64 + 0.5) * FRAME), Some(n), "frame {n}");
        }
    }

    #[test]
    fn a_far_seek_leaves_out_what_nothing_refers_to_and_lands_on_its_frame() {
        // One group of 40 pictures, longer than the kept ones, whose
        // B-frames nothing refers to.
        let path = fixture("h264-long.mp4");
        let track = container::read(&mut File::open(&path).unwrap()).unwrap();
        let (_, record) = track.extensions.iter().find(|(k, _)| k == b"avcC").unwrap();
        let length_size = avc::Record::parse(record).unwrap().length_size;
        let bytes = std::fs::read(&path).unwrap();
        let disposable = track
            .samples
            .iter()
            .filter(|s| {
                let start = s.offset as usize;
                avc::disposable(&bytes[start..start + s.size as usize], length_size)
            })
            .count();
        assert!(disposable >= 20, "{disposable} disposable samples");
        let middle = |n: usize| (n as f64 + 0.5) * FRAME;
        let frame = |c: &Clip, n| number(&c.frame_at(middle(n)).unwrap().unwrap(), 40, 5);
        for n in [39, 20, 38, 17, 0, 39, 30] {
            assert_eq!(frame(&clip("h264-long.mp4"), n), n, "frame {n} on its own");
        }
        let c = clip("h264-long.mp4");
        for n in [39, 20, 38, 17, 0, 39, 30, 31, 2] {
            assert_eq!(frame(&c, n), n, "frame {n} after the others");
        }
    }

    #[test]
    fn what_motion_encodes_reads_back_in_its_colours_and_times() {
        use motion_encode::{Codec, VideoQuality, VideoSpec, VideoWriter};
        // 1280 by 720, so the file's BT.709 tags and the size agree, at
        // 30 fps: four quadrants, and a bar whose place counts the frame.
        let (w, h) = (1280usize, 720usize);
        let path =
            std::env::temp_dir().join(format!("motion-video-own-{}.mp4", std::process::id()));
        let spec = VideoSpec {
            width: w as u32,
            height: h as u32,
            fps: 30.0,
        };
        let mut writer =
            VideoWriter::create(&path, spec, Codec::H264, VideoQuality::Master).unwrap();
        let pixel = |frame: usize, x: usize, y: usize| -> [u8; 4] {
            if y < 64 && x / 64 == frame {
                return [255, 255, 255, 255];
            }
            let q = usize::from(x >= w / 2) + 2 * usize::from(y >= h / 2);
            let [r, g, b] = QUADRANTS[q];
            [r, g, b, 255]
        };
        for frame in 0..12 {
            let rgba: Vec<u8> = (0..w * h)
                .flat_map(|i| pixel(frame, i % w, i / w))
                .collect();
            writer.push(&rgba).unwrap();
        }
        writer.finish().unwrap();
        let c = Clip::open(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!((c.seconds() - 0.4).abs() < 1e-9, "{}", c.seconds());
        for frame in [0, 5, 11, 3] {
            let f = c.frame_at(frame as f64 / 30.0).unwrap().unwrap();
            for (q, want) in QUADRANTS.iter().enumerate() {
                let (x, y) = ((q % 2) * w / 2 + w / 4, (q / 2) * h / 2 + h / 4);
                let got = &f.pixels[4 * (y * w + x)..][..3];
                let off = (0..3).map(|c| got[c].abs_diff(want[c])).max().unwrap();
                assert!(
                    off <= 2,
                    "frame {frame}, quadrant {q}: {got:?}, not {want:?}"
                );
            }
            // The bar is where this frame put it.
            let bar = (0..w / 64)
                .max_by_key(|&b| f.pixels[4 * (16 * w + b * 64 + 32)])
                .unwrap();
            assert_eq!(bar, frame, "frame {frame}");
        }
    }

    /// The colours of the quadrant clips, by quadrant.
    const QUADRANTS: [[u8; 3]; 4] = [[200, 60, 40], [40, 170, 80], [50, 80, 210], [230, 210, 60]];

    #[test]
    fn colour_follows_the_tags_and_untagged_clips_follow_their_size() {
        for name in [
            "h264-bt709.mp4",
            "h264-bt601.mp4",
            "h264-full.mp4",
            "h264-untagged-sd.mp4",
            "h264-untagged-hd.mp4",
            "h264-box-wins.mp4",
        ] {
            let frame = clip(name).frame_at(0.01).unwrap().unwrap();
            let (w, h) = (frame.width as usize, frame.height as usize);
            for (q, want) in QUADRANTS.iter().enumerate() {
                let (x, y) = ((q % 2) * w / 2 + w / 4, (q / 2) * h / 2 + h / 4);
                let at = 4 * (y * w + x);
                let got = &frame.pixels[at..at + 4];
                let off = (0..3).map(|c| got[c].abs_diff(want[c])).max().unwrap();
                assert!(
                    off <= 3 && got[3] == 255,
                    "{name}: quadrant {q} is {got:?}, not {want:?}"
                );
            }
        }
    }
}

/// Decoding speed on a clip of your own: forward through every frame, at
/// every twelfth frame, and at frames in a scattered order.
///
/// `MOTION_VIDEO_CLIP=clip.mp4 cargo test --release -p motion-video timing -- --ignored --nocapture`
#[test]
#[ignore = "times a clip named by MOTION_VIDEO_CLIP"]
fn timing() {
    use std::time::Instant;
    let Some(path) = std::env::var_os("MOTION_VIDEO_CLIP") else {
        return;
    };
    let path = PathBuf::from(path);
    let frames = {
        let mut file = File::open(&path).unwrap();
        container::read(&mut file).unwrap().samples.len()
    };
    let seconds = Clip::open(&path).unwrap().seconds();
    let time = |n: usize| (n as f64 + 0.5) * seconds / frames as f64;
    let run = |name: &str, order: &[usize]| {
        let start = Instant::now();
        let clip = Clip::open(&path).unwrap();
        let opened = start.elapsed();
        for &n in order {
            assert!(clip.frame_at(time(n)).unwrap().is_some(), "frame {n}");
        }
        let each = (start.elapsed() - opened).as_secs_f64() * 1e3 / order.len() as f64;
        eprintln!(
            "{name}: {} frames, {each:.2} ms a frame (open {:.1} ms)",
            order.len(),
            opened.as_secs_f64() * 1e3
        );
    };
    let all: Vec<usize> = (0..frames).collect();
    run("forward", &all);
    run(
        "every 12th",
        &all.iter().copied().step_by(12).collect::<Vec<_>>(),
    );
    // A fixed scatter: n * 7919 mod frames visits every frame once.
    let scattered: Vec<usize> = (0..frames.min(60)).map(|n| n * 7919 % frames).collect();
    run("scattered", &scattered);
}
