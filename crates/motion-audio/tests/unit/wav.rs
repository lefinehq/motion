//! Unit tests for `src/wav.rs`: written mixes read back, and broken files
//! say what is wrong.

use super::*;
use crate::open;

#[test]
fn a_written_mix_reads_back_the_same() {
    let sound = open(&crate::tests::data("f32.wav")).expect("decodes");
    let again = decode(&encode(&sound)).expect("reads back");
    assert_eq!(again, sound);
}

#[test]
fn a_data_chunk_running_past_the_end_is_read_to_it() {
    let sound = open(&crate::tests::data("s16.wav")).expect("decodes");
    let mut bytes = std::fs::read(crate::tests::data("s16.wav")).expect("reads");
    let data = bytes
        .windows(4)
        .position(|w| w == b"data")
        .expect("a data chunk");
    bytes[data + 4..data + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(decode(&bytes).expect("decodes"), sound);
}

#[test]
fn broken_files_say_what_is_wrong() {
    let header = |rest: &[u8]| [b"RIFF\0\0\0\0WAVE".as_slice(), rest].concat();
    let no_format = decode(&header(b"data\0\0\0\0")).expect_err("no fmt");
    assert!(no_format.message.contains("no `fmt `"), "{no_format}");
    let mut fmt = b"fmt \x10\0\0\0".to_vec();
    // ADPCM, 1 channel, 8000 Hz, blocks of 1 byte, 4 bits.
    fmt.extend_from_slice(&[2, 0, 1, 0, 0x40, 0x1F, 0, 0, 0x40, 0x1F, 0, 0, 1, 0, 4, 0]);
    let adpcm = decode(&header(&fmt)).expect_err("ADPCM");
    assert!(adpcm.message.contains("format code 0x0002"), "{adpcm}");
}
