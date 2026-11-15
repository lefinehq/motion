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
        (
            error.kind,
            error.path_text().as_deref(),
            error.expected.as_deref()
        ),
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

/// Where `needle` first is in `haystack`.
fn at(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("the bytes are there")
}

/// A file of nested lists, odd bodies and an XMP-like tail after the root.
fn nested() -> Vec<u8> {
    let mut bytes = file(&[
        chunk(b"head", &[7; 20]),
        list(
            b"Fold",
            &[
                chunk(b"fdta", &[1; 3]),
                list(b"Item", &[chunk(b"cdat", &[2; 16]), chunk(b"Utf8", b"odd")]),
            ],
        ),
        list(b"btdk", &[b"<< raw /text (Hello) >>".to_vec()]),
        chunk(b"tail", &[9; 2]),
    ]);
    bytes.extend_from_slice(b"<?xpacket?>");
    bytes
}

#[test]
fn writing_without_patches_gives_the_same_bytes() {
    let bytes = nested();
    assert_eq!(write(&bytes, &[]).unwrap(), bytes);
}

#[test]
fn a_patch_of_the_same_size_changes_only_its_bytes() {
    let bytes = nested();
    let start = at(&bytes, &[2; 16]) + 8;
    let patch = Patch {
        range: start..start + 4,
        bytes: vec![5; 4],
    };
    let written = write(&bytes, &[patch]).unwrap();
    let changed: Vec<usize> = (0..bytes.len())
        .filter(|&i| bytes[i] != written[i])
        .collect();
    assert_eq!(changed, (start..start + 4).collect::<Vec<_>>());
}

#[test]
fn a_patch_that_grows_a_body_resizes_every_list_around_it() {
    let bytes = nested();
    let start = at(&bytes, b"odd");
    let patch = Patch {
        range: start..start + 3,
        bytes: b"longer text".to_vec(),
    };
    let written = write(&bytes, &[patch]).unwrap();
    assert_eq!(
        written.len(),
        bytes.len() + 8,
        "8 more bytes, and no pad byte"
    );
    let root = read(&written).unwrap();
    let names: Vec<String> = root
        .chunks()
        .unwrap()
        .iter()
        .map(|c| c.name.to_string())
        .collect();
    assert_eq!(names, ["head", "LIST:Fold", "LIST:btdk", "tail"]);
    let fold = root.chunks().unwrap()[1];
    assert_eq!(tree(&fold).unwrap(), ["fdta", "LIST:Item", "cdat", "Utf8"]);
    let item = &root.chunks().unwrap()[1].chunks().unwrap()[1];
    assert_eq!(item.chunks().unwrap()[1].body, b"longer text");
    assert!(written.ends_with(b"<?xpacket?>"));
}

#[test]
fn a_list_of_raw_bytes_is_patched_as_bytes() {
    let bytes = nested();
    let start = at(&bytes, b"Hello");
    let patch = Patch {
        range: start..start + 5,
        bytes: b"Hi".to_vec(),
    };
    let written = write(&bytes, &[patch]).unwrap();
    let root = read(&written).unwrap();
    let btdk = root.chunks().unwrap()[2];
    assert_eq!(btdk.name.to_string(), "LIST:btdk");
    assert_eq!(btdk.body, b"<< raw /text (Hi) >>");
    assert_eq!(root.chunks().unwrap()[3].body, [9; 2]);
}

#[rstest]
#[case::overlapping(vec![(30, 40), (35, 45)], "overlap")]
#[case::outside_a_body(vec![(0, 2)], "outside")]
fn bad_patches_are_refused(#[case] ranges: Vec<(usize, usize)>, #[case] why: &str) {
    let bytes = nested();
    let patches: Vec<Patch> = ranges
        .into_iter()
        .map(|(a, b)| Patch {
            range: a..b,
            bytes: vec![0; b - a],
        })
        .collect();
    let error = write(&bytes, &patches).unwrap_err();
    assert!(error.message.contains(why), "{}", error.message);
}

proptest! {
    /// Any body of the tree, replaced by any bytes, reads back as those
    /// bytes, and every other chunk is as it was.
    #[test]
    fn any_body_can_be_replaced(which in 0usize..4, new in proptest::collection::vec(any::<u8>(), 0..40)) {
        let bytes = nested();
        let bodies: [&[u8]; 4] = [&[7; 20], &[1; 3], &[2; 16], &[9; 2]];
        let start = at(&bytes, bodies[which]);
        let patch = Patch { range: start..start + bodies[which].len(), bytes: new.clone() };
        let written = write(&bytes, &[patch]).unwrap();
        let root = read(&written).unwrap();
        let mut found = Vec::new();
        fn leaves<'a>(chunk: &Chunk<'a>, found: &mut Vec<(String, &'a [u8])>) {
            for child in chunk.chunks().unwrap() {
                match child.name.list {
                    Some(list) if list.0 != *b"btdk" => leaves(&child, found),
                    _ => found.push((child.name.to_string(), child.body)),
                }
            }
        }
        leaves(&root, &mut found);
        let expected: Vec<&[u8]> = (0..4).map(|i| if i == which { new.as_slice() } else { bodies[i] }).collect();
        let got: Vec<&[u8]> = found.iter().filter(|(n, _)| n != "Utf8" && n != "LIST:btdk").map(|(_, b)| *b).collect();
        prop_assert_eq!(got, expected);
    }
}
