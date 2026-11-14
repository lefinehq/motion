//! Unit tests for `src/rifx.rs`.

use proptest::prelude::*;
use rstest::rstest;

use super::*;
use crate::test_support::{chunk, file, list};

/// Every chunk below `chunk`, read to the bottom, as `name` lines.
fn tree(chunk: &Chunk<'_>) -> Result<Vec<String>> {
    let children = match chunk.name.list {
        Some(_) => chunk.chunks()?,
        None => Vec::new(),
    };
    let nested = children
        .iter()
        .map(|child| Ok([vec![child.name.to_string()], tree(child)?].concat()))
        .collect::<Result<Vec<_>>>()?;
    Ok(nested.concat())
}

#[test]
fn reads_chunks_and_lists_in_order() {
    let bytes = file(&[
        chunk(b"head", &[0; 20]),
        list(b"Fold", &[chunk(b"fdta", &[1; 3]), list(b"Item", &[])]),
        chunk(b"Utf8", b"name"),
    ]);
    let root = read(&bytes).unwrap();
    assert_eq!(root.name.to_string(), "RIFX:Egg!");
    assert_eq!(
        tree(&root).unwrap(),
        ["head", "LIST:Fold", "fdta", "LIST:Item", "Utf8"]
    );
}

#[test]
fn an_odd_body_is_followed_by_one_pad_byte() {
    let bytes = file(&[chunk(b"cdrp", &[1]), chunk(b"Utf8", b"abc")]);
    let children = read(&bytes).unwrap().chunks().unwrap();
    let bodies: Vec<&[u8]> = children.iter().map(|c| c.body).collect();
    assert_eq!(bodies, [b"\x01".as_slice(), b"abc"]);
}

#[test]
fn a_missing_final_pad_byte_is_tolerated() {
    let mut bytes = chunk(b"Utf8", b"abc");
    bytes.pop();
    let parent = Chunk {
        name: Name::list(b"Item"),
        body: &bytes,
    };
    assert_eq!(parent.chunks().unwrap()[0].body, b"abc");
}

#[test]
fn names_show_the_list_type_without_trailing_spaces() {
    assert_eq!(Name::list(b"Pin ").to_string(), "LIST:Pin");
    assert_eq!(Name::chunk(b"tdb4").to_string(), "tdb4");
    assert_eq!(format!("{:?}", Tag(*b"a\0b ")), "`a\\x00b`");
}

#[rstest]
#[case::riff(b"RIFF\0\0\0\x04Egg!".to_vec())]
#[case::short_form_type(chunk(b"RIFX", b"Egg"))]
#[case::other_form_type(chunk(b"RIFX", b"WAVE"))]
fn refuses_a_file_that_is_not_rifx(#[case] bytes: Vec<u8>) {
    let error = read(&bytes).unwrap_err();
    assert_eq!(
        (error.kind, error.expected.as_deref()),
        (
            Kind::Invalid,
            Some("a file that starts with `RIFX` and `Egg!`")
        )
    );
}

#[test]
fn refuses_a_chunk_past_the_end_of_its_list() {
    let mut bytes = file(&[chunk(b"head", &[0; 20])]);
    bytes.truncate(bytes.len() - 4);
    let error = read(&bytes).unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref(), error.expected.as_deref()),
        (
            Kind::Invalid,
            Some("RIFX"),
            Some("a chunk that ends inside its list")
        )
    );

    let short = [b"cdta".as_slice(), &9u32.to_be_bytes(), &[0; 4]].concat();
    let error = Chunk {
        name: Name::list(b"Item"),
        body: &short,
    }
    .chunks()
    .unwrap_err();
    assert_eq!(
        error.expected.as_deref(),
        Some("a chunk that ends inside its list")
    );
}

#[test]
fn refuses_a_truncated_header_or_list_type() {
    let parent = Chunk {
        name: Name::list(b"Item"),
        body: b"cdta\0\0",
    };
    assert_eq!(
        parent.chunks().unwrap_err().expected.as_deref(),
        Some("a whole `chunk header` record")
    );
    let bytes = chunk(b"LIST", b"Fo");
    let parent = Chunk {
        name: Name::list(b"Fold"),
        body: &bytes,
    };
    assert_eq!(
        parent.chunks().unwrap_err().expected.as_deref(),
        Some("a four-character list type")
    );
}

proptest! {
    #[test]
    fn never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = read(&bytes).and_then(|root| tree(&root));
    }

    #[test]
    fn never_panics_on_arbitrary_bodies(body in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let bytes = file(&[body]);
        let _ = read(&bytes).and_then(|root| tree(&root));
    }

    #[test]
    fn never_panics_on_arbitrary_lists(
        parts in proptest::collection::vec(
            (prop_oneof![Just(*b"LIST"), Just(*b"Utf8"), any::<[u8; 4]>()], 0u32..64, proptest::collection::vec(any::<u8>(), 0..64)),
            0..32,
        )
    ) {
        let soup: Vec<u8> = parts
            .iter()
            .flat_map(|(tag, size, bytes)| [tag.as_slice(), &size.to_be_bytes(), bytes].concat())
            .collect();
        let bytes = file(&[soup]);
        let _ = read(&bytes).and_then(|root| tree(&root));
    }
}
