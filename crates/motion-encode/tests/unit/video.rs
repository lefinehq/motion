//! Unit tests for `src/video.rs`.

use std::path::PathBuf;

use mp4_atom::{Decode, Ftyp, Moov, StszSamples};
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
#[case("a.mov", Some(Container::Mov))]
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
        true => "one of h264, hevc, prores, av1",
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

#[test]
fn av1_is_refused_in_quicktime() {
    let err = VideoWriter::create(
        &scratch("av1-mov").join("a.mov"),
        SPEC,
        Codec::Av1,
        VideoQuality::Web,
    )
    .err()
    .unwrap();
    assert_eq!(err.message, "AV1 in a `.mov` file");
    assert_eq!(err.expected.as_deref(), Some("a `.mp4` file for AV1"));
}

#[cfg(target_os = "macos")]
#[test]
fn prores_goes_in_quicktime_at_one_quality() {
    let dir = scratch("prores-rules");
    let err = VideoWriter::create(
        &dir.join("a.mp4"),
        SPEC,
        Codec::ProRes,
        VideoQuality::Master,
    )
    .err()
    .unwrap();
    assert_eq!(
        err.expected.as_deref(),
        Some("a `.mov` file for ProRes 4444")
    );
    let err = VideoWriter::create(&dir.join("a.mov"), SPEC, Codec::ProRes, VideoQuality::Web)
        .err()
        .unwrap();
    assert_eq!(err.message, "unknown video quality for ProRes 4444 `web`");
    assert_eq!(Codec::ProRes.default_quality(), VideoQuality::Master);
}

#[cfg(target_os = "macos")]
#[test]
fn prores_writes_a_quicktime_file_with_alpha() {
    let path = scratch("prores").join("clear.mov");
    let mut writer = VideoWriter::create(&path, SPEC, Codec::ProRes, VideoQuality::Master).unwrap();
    // A clear frame with an opaque bar.
    for frame in 0..3usize {
        let mut rgba = [0u8; 4].repeat(48 * 32);
        for y in 0..32 {
            rgba[(y * 48 + frame * 4) * 4..][..4].copy_from_slice(&[250, 200, 40, 255]);
        }
        writer.push(&rgba).unwrap();
    }
    assert_eq!(writer.finish().unwrap(), 3);
    let bytes = fs::read(&path).unwrap();
    let buf = &mut bytes.as_slice();
    let ftyp = Ftyp::decode(buf).unwrap();
    assert_eq!(ftyp.major_brand, b"qt  ".into());
    let moov = Moov::decode(buf).unwrap();
    let stbl = &moov.trak[0].mdia.minf.stbl;
    let mp4_atom::Codec::Unknown(kind, body) = &stbl.stsd.codecs[0] else {
        panic!("a ProRes sample entry");
    };
    assert_eq!(*kind, b"ap4h".into());
    // The visual fields' depth, 32 bits for alpha, sits 74 bytes in.
    assert_eq!(&body[74..76], &[0, 32]);
    assert_eq!(stbl.stss.as_ref().unwrap().entries, vec![1, 2, 3]);
}

/// A second of a quiet tone, stereo at 48 kHz.
fn tone() -> Soundtrack {
    let wave: Vec<f32> = (0..48_000)
        .map(|i| (0.2 * (std::f64::consts::TAU * 220.0 * f64::from(i) / 48_000.0).sin()) as f32)
        .collect();
    Soundtrack {
        rate: 48_000,
        channels: vec![wave.clone(), wave],
    }
}

/// The index of a video of 30 frames with [`tone`] as its sound.
#[cfg(target_os = "macos")]
fn with_sound(path: &Path, codec: Codec) -> (Moov, Vec<u8>) {
    let mut writer = VideoWriter::create(path, SPEC, codec, codec.default_quality()).unwrap();
    writer.set_sound(&tone()).unwrap();
    for _ in 0..30 {
        writer.push(&[20, 20, 60, 255].repeat(48 * 32)).unwrap();
    }
    writer.finish().unwrap();
    let bytes = fs::read(path).unwrap();
    let buf = &mut bytes.as_slice();
    Ftyp::decode(buf).unwrap();
    (Moov::decode(buf).unwrap(), bytes)
}

#[cfg(target_os = "macos")]
#[test]
fn a_quicktime_file_holds_its_sound_as_pcm_after_the_frames() {
    let (moov, bytes) = with_sound(&scratch("pcm").join("tone.mov"), Codec::H264);
    assert_eq!((moov.trak.len(), moov.mvhd.next_track_id), (2, 3));
    let sound = &moov.trak[1];
    assert_eq!(sound.mdia.hdlr.handler, b"soun".into());
    assert_eq!(
        (sound.mdia.mdhd.timescale, sound.mdia.mdhd.duration),
        (48_000, 48_000)
    );
    // One second in the movie's ticks, as long as the 30 frames.
    assert_eq!((sound.tkhd.duration, moov.mvhd.duration), (30_000, 30_000));
    let stbl = &sound.mdia.minf.stbl;
    let mp4_atom::Codec::Sowt(sowt) = &stbl.stsd.codecs[0] else {
        panic!("16-bit PCM");
    };
    assert_eq!((sowt.audio.channel_count, sowt.audio.sample_size), (2, 16));
    // The samples end the file.
    let start = stbl.stco.as_ref().unwrap().entries[0] as usize;
    assert_eq!(bytes.len() - start, 48_000 * 4);
}

#[cfg(target_os = "macos")]
#[test]
fn an_mp4_file_holds_aac_with_its_priming_edited_out() {
    let (moov, bytes) = with_sound(&scratch("aac").join("tone.mp4"), Codec::H264);
    let sound = &moov.trak[1];
    let stbl = &sound.mdia.minf.stbl;
    let mp4_atom::Codec::Mp4a(mp4a) = &stbl.stsd.codecs[0] else {
        panic!("AAC");
    };
    let config = &mp4a.esds.es_desc.dec_config;
    assert_eq!(
        (config.object_type_indication, config.avg_bitrate),
        (0x40, 192_000)
    );
    let elst = sound.edts.as_ref().unwrap().elst.as_ref().unwrap();
    let edit = &elst.entries[0];
    assert!(edit.media_time.is_some_and(|t| t > 0), "{edit:?}");
    assert_eq!(edit.segment_duration, 30_000);
    let StszSamples::Different { sizes } = &stbl.stsz.samples else {
        panic!("packets of their own sizes");
    };
    let start = stbl.stco.as_ref().unwrap().entries[0] as usize;
    assert_eq!(bytes.len() - start, sizes.iter().sum::<u32>() as usize);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn an_mp4_file_without_an_aac_encoder_refuses_sound() {
    let path = scratch("no-aac").join("tone.mp4");
    let mut writer = VideoWriter::create(&path, SPEC, Codec::Av1, VideoQuality::Web).unwrap();
    let refused = writer.set_sound(&tone()).unwrap_err();
    assert!(refused.to_string().contains(".mov"), "{refused}");
}
