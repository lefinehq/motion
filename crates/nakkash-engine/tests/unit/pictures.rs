//! Unit tests for `src/pictures.rs`.

use std::fs;

use nakkash_format::Kind;
use nakkash_raster::{Image, Picture};

use super::*;

/// A fresh scratch folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "nakkash-engine-pictures-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create a scratch folder");
    dir
}

/// A 3x2 opaque PNG.
fn png() -> Vec<u8> {
    Image::new(3, 2, [10, 20, 30, 255].repeat(6))
        .unwrap()
        .to_png()
        .unwrap()
}

/// The raster picture `still` holds.
fn raster(still: Still) -> Picture {
    match still {
        Still::Picture(picture) => picture,
        Still::Drawing(drawing) => panic!("a picture, not {drawing:?}"),
    }
}

/// A one-page PDF, 30 by 10 points, with a red square.
fn pdf() -> Vec<u8> {
    crate::test_support::pdf(30, 10, "", "", "1 0 0 rg 0 0 10 10 re f")
}

#[test]
fn a_picture_is_read_and_decoded_once() {
    let path = scratch("once").join("a.png");
    fs::write(&path, png()).unwrap();
    let pictures = Pictures::default();
    let first = raster(pictures.get(&path).unwrap());
    assert_eq!((first.width(), first.height()), (3, 2));
    // Gone from the disk, it is still drawn from what was decoded.
    fs::remove_file(&path).unwrap();
    assert_eq!(raster(pictures.get(&path).unwrap()), first);
}

#[test]
fn vector_footage_is_known_by_its_first_bytes_whatever_its_name() {
    let path = scratch("vector").join("logo.png");
    fs::write(&path, pdf()).unwrap();
    let Still::Drawing(drawing) = Pictures::default().get(&path).unwrap() else {
        panic!("a drawing");
    };
    assert_eq!(drawing.format(), "PDF");
    assert_eq!((drawing.size().width, drawing.size().height), (30.0, 10.0));
}

#[test]
fn vector_footage_that_cannot_be_read_names_its_path() {
    let path = scratch("broken").join("logo.ai");
    fs::write(&path, "%PDF-1.7\nno objects").unwrap();
    let e = Pictures::default().get(&path).unwrap_err();
    assert_eq!(e.kind, Kind::Invalid);
    assert!(e.message.contains(&path.display().to_string()), "{e}");
}

#[test]
fn a_missing_file_names_its_path_and_what_was_expected() {
    let path = scratch("missing").join("nowhere.jpg");
    let e = Pictures::default().get(&path).unwrap_err();
    assert_eq!(e.kind, Kind::Io);
    assert!(e.message.contains(&path.display().to_string()), "{e}");
    assert_eq!(
        e.expected.as_deref(),
        Some("a PNG, JPEG or PDF file, relative to the scene file's folder")
    );
}

#[test]
fn a_file_that_is_not_a_picture_names_its_path_and_the_formats() {
    let path = scratch("text").join("notes.png");
    fs::write(&path, "not a picture").unwrap();
    let e = Pictures::default().get(&path).unwrap_err();
    assert_eq!(e.kind, Kind::Invalid);
    assert!(e.message.contains(&path.display().to_string()), "{e}");
    assert_eq!(e.expected.as_deref(), Some("a PNG, JPEG or PDF file"));
}
