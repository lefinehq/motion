//! Unit tests for `src/encoder.rs`, on the Mac's own encoders.

use rstest::rstest;

use super::*;

const WIDTH: usize = 128;
const HEIGHT: usize = 72;

fn settings(codec: Codec) -> Settings {
    Settings {
        codec,
        width: WIDTH as u32,
        height: HEIGHT as u32,
        fps: 30.0,
        quality: 0.6,
        keyframe_interval: 240,
    }
}

/// Encodes `count` frames of a moving bar and returns every packet.
fn encode(encoder: &mut Encoder, count: u64) -> Vec<Packet> {
    let chroma = WIDTH.div_ceil(2) * HEIGHT.div_ceil(2);
    let (cb, cr) = (vec![90u8; chroma], vec![160u8; chroma]);
    let mut packets = Vec::new();
    for frame in 0..count {
        let y: Vec<u8> = (0..WIDTH * HEIGHT)
            .map(|i| {
                if (i % WIDTH) / 8 == frame as usize % 16 {
                    235
                } else {
                    40
                }
            })
            .collect();
        let planes = Planes {
            y: &y,
            cb: &cb,
            cr: &cr,
        };
        packets.extend(encoder.encode(planes).unwrap());
    }
    packets.extend(encoder.finish().unwrap());
    packets
}

#[rstest]
#[case::h264(Codec::H264)]
#[case::hevc(Codec::Hevc)]
fn every_frame_comes_out_in_order_with_its_configuration(#[case] codec: Codec) {
    let mut encoder = Encoder::new(settings(codec)).unwrap();
    let packets = encode(&mut encoder, 10);
    let frames: Vec<u64> = packets.iter().map(|p| p.frame).collect();
    assert_eq!(frames, (0..10).collect::<Vec<_>>());
    assert!(packets[0].keyframe);
    assert!(packets.iter().all(|p| !p.data.is_empty()));
    // Each packet is NAL units after four-byte lengths that add up.
    for packet in &packets {
        let mut rest = &packet.data[..];
        while let [a, b, c, d, tail @ ..] = rest {
            let length = u32::from_be_bytes([*a, *b, *c, *d]) as usize;
            rest = &tail[length..];
        }
        assert!(rest.is_empty());
    }
    // Both records start with configuration version 1.
    let record = encoder.config().expect("a configuration record");
    assert_eq!(record[0], 1);
}

#[test]
fn planes_of_another_size_are_refused() {
    let mut encoder = Encoder::new(settings(Codec::H264)).unwrap();
    let planes = Planes {
        y: &[0; 4],
        cb: &[0; 1],
        cr: &[0; 1],
    };
    let error = encoder.encode(planes).unwrap_err();
    assert_eq!(error.doing, "taking planes of another size");
}
