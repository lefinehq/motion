//! Unit tests for `src/mp4.rs`.

use std::path::PathBuf;

use pretty_assertions::assert_eq;

use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("motion-encode-mp4-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir.join("out.mp4")
}

const SPEC: VideoSpec = VideoSpec {
    width: 64,
    height: 32,
    fps: 25.0,
};

fn packet(frame: u64, keyframe: bool) -> Packet {
    Packet {
        data: vec![frame as u8; 10 + frame as usize],
        frame,
        keyframe,
    }
}

fn config() -> DecoderConfig {
    // A record for 8-bit 4:2:0 main profile with no configuration OBUs.
    DecoderConfig::Av1 {
        record: vec![0x81, 0x1f, 0x0c, 0x00],
    }
}

#[test]
fn packets_out_of_order_are_refused() {
    let path = scratch("order");
    let mut mp4 = Mp4::create(&path, SPEC, Container::Mp4).unwrap();
    mp4.write(&packet(0, true)).unwrap();
    let err = mp4.write(&packet(2, false)).unwrap_err();
    assert_eq!(err.kind, Kind::Invalid);
    assert!(err.message.contains("frame 2"), "{}", err.message);
}

#[test]
fn the_index_comes_first_and_points_at_the_samples() {
    let path = scratch("layout");
    let mut mp4 = Box::new(Mp4::create(&path, SPEC, Container::Mp4).unwrap());
    for frame in 0..5 {
        mp4.write(&packet(frame, frame % 3 == 0)).unwrap();
    }
    mp4.finish(&config()).unwrap();
    assert!(
        !path.with_extension("mp4.mdat").exists(),
        "the scratch file is removed"
    );

    let bytes = fs::read(&path).unwrap();
    let buf = &mut bytes.as_slice();
    let ftyp = Ftyp::decode(buf).unwrap();
    assert!(ftyp.compatible_brands.contains(&b"av01".into()));
    let moov = Moov::decode(buf).unwrap();
    assert_eq!(&buf[4..8], b"mdat");

    let stbl = &moov.trak[0].mdia.minf.stbl;
    let StszSamples::Different { sizes } = &stbl.stsz.samples else {
        panic!("sizes differ");
    };
    assert_eq!(sizes, &[10, 11, 12, 13, 14]);
    assert_eq!(stbl.stss.as_ref().unwrap().entries, vec![1, 4]);
    let offset = stbl.stco.as_ref().unwrap().entries[0] as usize;
    assert_eq!(&bytes[offset..offset + 10], &[0; 10]);
    assert_eq!(bytes.len(), offset + 60);
    assert_eq!(moov.mvhd.timescale, 25_000);
    assert_eq!(moov.mvhd.duration, 5_000);
}

#[test]
fn offsets_past_four_gib_take_64_bits() {
    let mp4 = Mp4::create(&scratch("co64"), SPEC, Container::Mp4).unwrap();
    let stbl = mp4
        .moov(&config(), 1 << 33)
        .unwrap()
        .trak
        .remove(0)
        .mdia
        .minf
        .stbl;
    assert_eq!(
        (stbl.stco, stbl.co64),
        (
            None,
            Some(Co64 {
                entries: vec![1 << 33]
            })
        )
    );
}

#[test]
fn a_record_that_does_not_read_is_an_error() {
    assert!(record_box::<mp4_atom::Av1c>(b"av1C", &[0x00]).is_err());
}

#[test]
fn the_prores_entry_is_renamed_in_place() {
    let path = scratch("rename");
    let mut mp4 = Mp4::create(&path, SPEC, Container::Mov).unwrap();
    mp4.sizes = vec![10];
    let mut bytes = Vec::new();
    encode(&mp4.moov(&DecoderConfig::ProRes, 100).unwrap(), &mut bytes).unwrap();
    let length = bytes.len();
    assert!(rename_entry(&mut bytes, b"jpeg", b"ap4h"));
    assert_eq!(bytes.len(), length);
    // Renamed once, it is no longer a `jpeg` entry to rename.
    assert!(!rename_entry(&mut bytes, b"jpeg", b"ap4h"));
    let moov = Moov::decode(&mut bytes.as_slice()).unwrap();
    let mp4_atom::Codec::Unknown(kind, _) = &moov.trak[0].mdia.minf.stbl.stsd.codecs[0] else {
        panic!("an entry mp4-atom does not know");
    };
    assert_eq!(*kind, b"ap4h".into());
}

#[test]
fn renaming_needs_the_whole_path() {
    let mut bytes = vec![
        0, 0, 0, 16, b'm', b'o', b'o', b'v', 0, 0, 0, 8, b'f', b'r', b'e', b'e',
    ];
    assert!(!rename_entry(&mut bytes, b"jpeg", b"ap4h"));
    // A box that claims more than its parent holds ends the walk.
    let mut bytes = vec![
        0, 0, 0, 16, b'm', b'o', b'o', b'v', 0, 0, 0, 99, b't', b'r', b'a', b'k',
    ];
    assert!(!rename_entry(&mut bytes, b"jpeg", b"ap4h"));
}
