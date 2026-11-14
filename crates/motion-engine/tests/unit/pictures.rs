//! Unit tests for `src/pictures.rs`.

use std::fs;

use motion_format::Kind;
use motion_raster::Image;

use super::*;

/// A fresh scratch folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "motion-engine-pictures-{}-{name}",
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

#[test]
fn a_picture_is_read_and_decoded_once() {
    let path = scratch("once").join("a.png");
    fs::write(&path, png()).unwrap();
    let pictures = Pictures::default();
    let first = pictures.get(&path).unwrap();
    assert_eq!((first.width(), first.height()), (3, 2));
    // Gone from the disk, it is still drawn from what was decoded.
    fs::remove_file(&path).unwrap();
    assert_eq!(pictures.get(&path).unwrap(), first);
}

#[test]
fn a_missing_file_names_its_path_and_what_was_expected() {
    let path = scratch("missing").join("nowhere.jpg");
    let e = Pictures::default().get(&path).unwrap_err();
    assert_eq!(e.kind, Kind::Io);
    assert!(e.message.contains(&path.display().to_string()), "{e}");
    assert_eq!(
        e.expected.as_deref(),
        Some("a PNG or JPEG file, relative to the scene file's folder")
    );
}

#[test]
fn a_file_that_is_not_a_picture_names_its_path_and_the_formats() {
    let path = scratch("text").join("notes.png");
    fs::write(&path, "not a picture").unwrap();
    let e = Pictures::default().get(&path).unwrap_err();
    assert_eq!(e.kind, Kind::Invalid);
    assert!(e.message.contains(&path.display().to_string()), "{e}");
    assert_eq!(e.expected.as_deref(), Some("a PNG or JPEG file"));
}
