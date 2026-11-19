use std::fs;

use super::*;

/// A store in a new folder of its own.
pub(crate) fn scratch(name: &str) -> Store {
    let root = std::env::temp_dir().join(format!("nakkash-baked-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    Store::new(root)
}

#[test]
fn gives_back_what_it_keeps() {
    let store = scratch("keep");
    let hash = store.put(b"baked").unwrap();
    assert_eq!(hash, Sha256::of(b"baked"));
    assert_eq!(store.put(b"baked").unwrap(), hash, "kept once");
    assert_eq!(store.get(&hash).unwrap().unwrap(), b"baked");
    assert!(store.get(&Sha256::of(b"absent")).unwrap().is_none());
    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn copies_a_file_under_its_hash() {
    let store = scratch("copy");
    let source = store.root().with_extension("source");
    fs::write(&source, b"model bytes").unwrap();
    let (hash, size) = store.put_file(&source, None).unwrap();
    assert_eq!((hash, size), (Sha256::of(b"model bytes"), 11));
    assert_eq!(store.checked_path(&hash).unwrap(), Some(store.path(&hash)));
    // Again, with the hash it should have: the same file, kept once.
    assert_eq!(store.put_file(&source, Some(&hash)).unwrap(), (hash, 11));
    fs::remove_file(&source).unwrap();
    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn refuses_bytes_that_are_not_the_expected_file() {
    let store = scratch("expected");
    let wanted = Sha256::of(b"right");
    let error = store.put_reader(&b"wrong"[..], Some(&wanted)).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert!(!store.has(&wanted));
    assert!(!store.has(&Sha256::of(b"wrong")), "nothing kept");
    let left: Vec<_> = fs::read_dir(store.root()).unwrap().collect();
    assert!(left.is_empty(), "no partial file left: {left:?}");
    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn refuses_a_file_that_has_changed() {
    let store = scratch("changed");
    let hash = store.put(b"original").unwrap();
    fs::write(store.path(&hash), b"replaced").unwrap();
    assert_eq!(store.get(&hash).unwrap_err().kind, Kind::Invalid);
    let error = store.checked_path(&hash).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert!(error.message.contains("has changed"), "{}", error.message);
    assert!(
        store
            .checked_path(&Sha256::of(b"absent"))
            .unwrap()
            .is_none()
    );
    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn names_the_file_it_cannot_read() {
    let store = scratch("unreadable");
    let missing = store.root().join("no-such-file");
    let error = store.put_file(&missing, None).unwrap_err();
    assert_eq!(error.kind, Kind::Io);
}
