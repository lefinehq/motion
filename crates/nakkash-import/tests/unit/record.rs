//! Unit tests for `src/record.rs`.

use super::*;

#[test]
fn parse_reads_big_endian_fields() {
    let header: Header = parse(b"cdta\0\0\0\xCCrest", "chunk header", ()).unwrap();
    assert_eq!((header.tag, header.size), (Tag(*b"cdta"), 204));
}

#[test]
fn parse_names_a_record_that_is_too_short() {
    let error = parse::<Header>(b"cdta", "chunk header", ()).unwrap_err();
    assert_eq!(
        (error.kind, error.path, error.expected.as_deref()),
        (Kind::Invalid, None, Some("a whole `chunk header` record"))
    );
}
