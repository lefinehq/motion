//! Unit tests for `src/lib.rs`: the format table.

use crate::sample::page;

use super::*;

#[test]
fn a_pdf_is_recognised_from_its_first_bytes_alone() {
    let bytes = page("", "", vec![]);
    assert_eq!(
        recognise(&bytes[..HEAD.min(bytes.len())]).map(|f| f.name),
        Some("PDF")
    );
    assert_eq!(read(&bytes).unwrap().format(), "PDF");
}

#[test]
fn a_file_in_no_vector_format_names_the_formats() {
    assert!(recognise(b"\x89PNG\r\n\x1a\n").is_none());
    let e = read(b"plain text").unwrap_err();
    assert_eq!(e.kind, Kind::Invalid);
    assert_eq!(e.expected.as_deref(), Some("a PDF file"));
}
