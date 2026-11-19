use std::io::Cursor;

use super::*;

#[test]
fn hashes_as_the_standard_says() {
    // FIPS 180-2, appendix B.1.
    assert_eq!(
        Sha256::of(b"abc").to_string(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn reads_what_it_writes() {
    let hash = Sha256::of(b"nakkash");
    assert_eq!(hash.to_string().parse::<Sha256>().unwrap(), hash);
}

#[test]
fn rejects_what_is_not_a_hash() {
    for text in ["", "ab", &"g".repeat(64), &"é".repeat(32)] {
        let error = text.parse::<Sha256>().unwrap_err();
        assert_eq!(error.kind, Kind::Invalid, "{text}");
    }
}

#[test]
fn hashes_a_stream_as_it_hashes_its_bytes() {
    // Longer than one chunk, so the stream is read in pieces.
    let bytes: Vec<u8> = (0..CHUNK * 2 + 17).map(|i| (i % 251) as u8).collect();
    let streamed = Sha256::of_reader(Cursor::new(&bytes)).unwrap();
    assert_eq!(streamed, Sha256::of(&bytes));
}

#[test]
fn hashes_a_file_and_counts_its_bytes() {
    let path = std::env::temp_dir().join(format!("nakkash-sha256-{}", std::process::id()));
    std::fs::write(&path, b"abc").unwrap();
    let (hash, size) = Sha256::of_file(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!((hash, size), (Sha256::of(b"abc"), 3));
    let error = Sha256::of_file(&path).unwrap_err();
    assert_eq!(error.kind, Kind::Io);
}

#[test]
fn hashes_in_pieces_as_it_hashes_the_whole() {
    let mut hasher = Sha256Hasher::default();
    hasher.update(b"ab");
    hasher.update(b"");
    hasher.update(b"c");
    assert_eq!(hasher.finish(), Sha256::of(b"abc"));
}
