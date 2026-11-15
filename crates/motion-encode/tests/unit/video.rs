//! Unit tests for `src/video.rs`.

use std::path::PathBuf;

use mp4_atom::{Decode, Ftyp, Moov};
use pretty_assertions::assert_eq;
use rstest::rstest;

use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("motion-encode-video-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

const SPEC: VideoSpec = VideoSpec {
    width: 48,
    height: 32,
    fps: 30.0,
};

/// Writes `count` frames of a moving bar and returns the file's bytes.
fn write(path: &Path, count: u32) -> Vec<u8> {
    write_in(path, count, Codec::Av1)
}

/// Writes `count` frames of a moving bar in `codec`.
fn write_in(path: &Path, count: u32, codec: Codec) -> Vec<u8> {
    let mut writer = VideoWriter::create(path, SPEC, codec, VideoQuality::Web).unwrap();
    for frame in 0..count {
        let mut rgba = [20, 20, 60, 255].repeat(48 * 32);
        for y in 0..32 {
            let x = (frame * 4 + y) as usize % 48;
            rgba[(y as usize * 48 + x) * 4..][..4].copy_from_slice(&[250, 200, 40, 255]);
        }
        writer.push(&rgba).unwrap();
    }
    assert_eq!(writer.finish().unwrap(), u64::from(count));
    fs::read(path).unwrap()
}

#[rstest]
#[case("a.mp4", Some(Container::Mp4))]
#[case("A.MP4", Some(Container::Mp4))]
#[case("a.webm", None)]
#[case("frames", None)]
fn the_extension_picks_the_container(#[case] path: &str, #[case] expected: Option<Container>) {
    assert_eq!(Container::of(Path::new(path)), expected);
}

#[test]
fn an_unknown_extension_names_the_known_ones() {
    let err = VideoWriter::create(
        &scratch("ext").join("a.avi"),
        SPEC,
        Codec::Av1,
        VideoQuality::Web,
    )
    .err()
    .unwrap();
    assert_eq!(err.kind, Kind::UnknownName);
    assert!(err.message.contains("avi"), "{}", err.message);
}

#[rstest]
#[case::no_rate(VideoSpec { fps: 0.0, ..SPEC })]
#[case::no_width(VideoSpec { width: 0, ..SPEC })]
fn an_empty_spec_is_refused(#[case] spec: VideoSpec) {
    let err = VideoWriter::create(
        &scratch("spec").join("a.mp4"),
        spec,
        Codec::Av1,
        VideoQuality::Web,
    )
    .err()
    .unwrap();
    assert_eq!(err.kind, Kind::Invalid);
}

#[test]
fn a_frame_of_another_size_is_refused() {
    let path = scratch("size").join("a.mp4");
    let mut writer = VideoWriter::create(&path, SPEC, Codec::Av1, VideoQuality::Web).unwrap();
    let err = writer.push(&[0; 16]).unwrap_err();
    assert!(err.message.contains("16 bytes"), "{}", err.message);
}

#[test]
fn a_video_reads_back_with_every_frame() {
    let path = scratch("read").join("bar.mp4");
    let bytes = write(&path, 12);
    assert!(!partial_path(&path).exists());

    let buf = &mut bytes.as_slice();
    Ftyp::decode(buf).unwrap();
    let moov = Moov::decode(buf).unwrap();
    let track = &moov.trak[0];
    assert_eq!(
        (track.tkhd.width.integer(), track.tkhd.height.integer()),
        (48, 32)
    );
    assert_eq!(
        (track.mdia.mdhd.timescale, track.mdia.mdhd.duration),
        (30_000, 12_000)
    );
    let stbl = &track.mdia.minf.stbl;
    assert_eq!(stbl.stts.entries[0].sample_count, 12);
    assert_eq!(stbl.stss.as_ref().unwrap().entries[0], 1);
    let mp4_atom::Codec::Av01(av01) = &stbl.stsd.codecs[0] else {
        panic!("an AV1 sample entry");
    };
    assert!(
        !av01.av1c.config_obus.is_empty(),
        "the sequence header is stored"
    );
}

#[test]
fn the_same_frames_give_the_same_file() {
    let dir = scratch("same");
    assert_eq!(write(&dir.join("a.mp4"), 6), write(&dir.join("b.mp4"), 6));
}

#[test]
fn a_codec_this_build_lacks_names_the_ones_it_has() {
    assert_eq!(Codec::named("av1").unwrap(), Codec::Av1);
    let err = Codec::named("vp9").unwrap_err();
    assert_eq!(err.kind, Kind::UnknownName);
    assert_eq!(err.message, "unknown codec `vp9`");
    let expected = match cfg!(target_os = "macos") {
        true => "one of h264, hevc, av1",
        false => "one of av1",
    };
    assert_eq!(err.expected.as_deref(), Some(expected));
}

#[test]
fn the_default_codec_is_the_platforms_own() {
    let expected = match cfg!(target_os = "macos") {
        true => "h264",
        false => "av1",
    };
    assert_eq!(Codec::default().to_string(), expected);
}

#[test]
fn qualities_are_named_by_use() {
    assert_eq!(VideoQuality::named("master").unwrap(), VideoQuality::Master);
    assert_eq!(VideoQuality::default(), VideoQuality::Web);
    let err = VideoQuality::named("high").unwrap_err();
    assert_eq!(err.expected.as_deref(), Some("one of draft, web, master"));
}

#[cfg(target_os = "macos")]
#[rstest]
#[case::h264(Codec::H264)]
#[case::hevc(Codec::Hevc)]
fn the_systems_codecs_write_their_sample_entries(#[case] codec: Codec) {
    let path = scratch(&format!("system-{codec}")).join("bar.mp4");
    let bytes = write_in(&path, 12, codec);
    let buf = &mut bytes.as_slice();
    Ftyp::decode(buf).unwrap();
    let moov = Moov::decode(buf).unwrap();
    let stbl = &moov.trak[0].mdia.minf.stbl;
    assert_eq!(stbl.stts.entries[0].sample_count, 12);
    assert_eq!(stbl.stss.as_ref().unwrap().entries[0], 1);
    match (&stbl.stsd.codecs[0], codec) {
        (mp4_atom::Codec::Avc1(avc1), Codec::H264) => {
            assert!(!avc1.avcc.sequence_parameter_sets.is_empty())
        }
        (mp4_atom::Codec::Hvc1(_), Codec::Hevc) => {}
        (entry, _) => panic!("{codec} wrote {entry:?}"),
    }
}
