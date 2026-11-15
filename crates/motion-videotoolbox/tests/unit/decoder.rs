//! Unit tests for `src/decoder.rs`: what the Mac's encoders make, its
//! decoders give back.

use rstest::rstest;

use crate::{Codec, Encoder, Image, Planes, Settings};

use super::*;

const WIDTH: usize = 96;
const HEIGHT: usize = 54;

/// The luma of frame `frame`: a bar at a place of its own over a ramp.
fn luma(frame: usize) -> Vec<u8> {
    (0..WIDTH * HEIGHT)
        .map(|i| {
            let (x, y) = (i % WIDTH, i / WIDTH);
            if x / 8 == frame % 12 {
                220
            } else {
                (40 + y * 2) as u8
            }
        })
        .collect()
}

/// `count` frames encoded in `codec`, and the stream's configuration.
fn encoded(codec: Codec, count: usize) -> (Vec<Vec<u8>>, Vec<u8>) {
    let (packets, config) = packets(codec, count);
    (packets.into_iter().map(|p| p.data).collect(), config)
}

/// `count` frames encoded in `codec`, as packets, and the configuration.
fn packets(codec: Codec, count: usize) -> (Vec<crate::Packet>, Vec<u8>) {
    let mut encoder = Encoder::new(Settings {
        codec,
        width: WIDTH as u32,
        height: HEIGHT as u32,
        fps: 30.0,
        quality: 0.9,
        keyframe_interval: 5,
    })
    .unwrap();
    let chroma = WIDTH.div_ceil(2) * HEIGHT.div_ceil(2);
    let (cb, cr) = (vec![100u8; chroma], vec![150u8; chroma]);
    let mut packets = Vec::new();
    for frame in 0..count {
        let y = luma(frame);
        let planes = Planes {
            y: &y,
            cb: &cb,
            cr: &cr,
        };
        packets.extend(encoder.encode(Image::Yuv420(planes)).unwrap());
    }
    packets.extend(encoder.finish().unwrap());
    let config = encoder.config().unwrap();
    (packets, config)
}

fn record(codec: Codec, config: &[u8]) -> Record<'_> {
    match codec {
        Codec::H264 => Record::Avc(config),
        _ => Record::Hevc(config),
    }
}

/// The mean difference of the picture's luma from `want`, and its chroma
/// at the centre.
fn compare(picture: &Picture, want: &[u8]) -> (f64, (u8, u8)) {
    picture
        .read(|samples| {
            let Samples::Eight(p) = samples else {
                panic!("eight-bit samples");
            };
            assert_eq!((p.width, p.height), (WIDTH, HEIGHT));
            let mut sum = 0.0;
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    sum += f64::from(p.luma[y * p.luma_stride + x].abs_diff(want[y * WIDTH + x]));
                }
            }
            let at = (HEIGHT / 4) * p.chroma_stride + WIDTH / 2;
            (
                sum / (WIDTH * HEIGHT) as f64,
                (p.chroma[at], p.chroma[at + 1]),
            )
        })
        .unwrap()
}

#[rstest]
#[case::h264(Codec::H264)]
#[case::hevc(Codec::Hevc)]
fn every_sample_decodes_to_its_frame_in_order(#[case] codec: Codec) {
    let (samples, config) = encoded(codec, 12);
    assert_eq!(samples.len(), 12);
    let mut decoder = Decoder::new(record(codec, &config), WIDTH as u32, HEIGHT as u32).unwrap();
    for (frame, sample) in samples.iter().enumerate() {
        let picture = decoder.decode(sample).unwrap().expect("a picture");
        assert_eq!(picture.size(), (WIDTH as u32, HEIGHT as u32));
        let (mean, (cb, cr)) = compare(&picture, &luma(frame));
        assert!(mean < 2.0, "frame {frame}: luma off by {mean} on average");
        assert!(
            cb.abs_diff(100) <= 2 && cr.abs_diff(150) <= 2,
            "frame {frame}: chroma ({cb}, {cr})"
        );
    }
}

#[test]
fn decoding_starts_again_at_a_keyframe() {
    let (packets, config) = packets(Codec::H264, 12);
    let key = (1..12)
        .find(|&i| packets[i].keyframe)
        .expect("a second keyframe");
    let mut decoder = Decoder::new(Record::Avc(&config), WIDTH as u32, HEIGHT as u32).unwrap();
    // The decoder jumps from frame 1 to the next keyframe, then back to 0.
    for frame in [0, 1, key, key + 1, 0, 1] {
        let picture = decoder.decode(&packets[frame].data).unwrap().unwrap();
        let (mean, _) = compare(&picture, &luma(frame));
        assert!(mean < 2.0, "frame {frame}: luma off by {mean} on average");
    }
}

#[test]
fn pictures_outlive_the_decoder_and_cross_threads() {
    let (samples, config) = encoded(Codec::H264, 3);
    let mut decoder = Decoder::new(Record::Avc(&config), WIDTH as u32, HEIGHT as u32).unwrap();
    let pictures: Vec<Picture> = samples
        .iter()
        .map(|s| decoder.decode(s).unwrap().unwrap())
        .collect();
    drop(decoder);
    let means = std::thread::spawn(move || {
        pictures
            .iter()
            .enumerate()
            .map(|(frame, p)| compare(p, &luma(frame)).0)
            .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    assert!(means.iter().all(|&m| m < 2.0), "{means:?}");
}

#[test]
fn a_broken_configuration_or_sample_is_an_error() {
    let (samples, config) = encoded(Codec::H264, 1);
    // The system may take a record it cannot use, and refuse its samples.
    let broken = Decoder::new(Record::Avc(&[1, 2, 3]), 64, 32)
        .and_then(|mut decoder| decoder.decode(&samples[0]));
    assert!(broken.is_err());
    let mut decoder = Decoder::new(Record::Avc(&config), WIDTH as u32, HEIGHT as u32).unwrap();
    let e = decoder.decode(&[]).unwrap_err();
    assert!(e.doing.contains("empty"), "{e}");
    // NAL units whose lengths run past the sample.
    assert!(decoder.decode(&[0, 0, 1, 0, 0x65, 1, 2]).is_err());
    // The decoder still decodes after a failure.
    assert!(decoder.decode(&samples[0]).unwrap().is_some());
}
