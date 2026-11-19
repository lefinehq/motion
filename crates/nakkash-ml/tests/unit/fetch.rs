use std::io::Write;
use std::process;

use super::*;

fn scratch(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("nakkash-fetch-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

/// A zip with `members`, stored without compression.
fn package(path: &Path, members: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in members {
        zip.start_file(*name, stored).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn checks_a_package_against_its_registry_hash() {
    let root = scratch("sha512");
    let path = root.join("p");
    fs::write(&path, b"abc").unwrap();
    // FIPS 180-2, appendix C.1, in base64.
    let abc =
        "3a81oZNherrMQXNJriBBMRLm+k6JqX6iCp7u5ktV05ohkpkqJ0/BqDa6PCOj/uu9RU1EI2Q86A4qmslPpUyknw==";
    check_sha512(&path, abc).unwrap();
    let error = check_sha512(&path, "other").unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert_eq!(
        check_sha512(&root.join("none"), abc).unwrap_err().kind,
        Kind::Io
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn takes_the_locked_member_out_of_a_package() {
    let root = scratch("member");
    let path = root.join("p.zip");
    package(&path, &[("runtimes/x/lib.so", b"library"), ("other", b"x")]);
    let store = Store::new(root.join("files"));
    let member = Member {
        path: "runtimes/x/lib.so".into(),
        sha256: Sha256::of(b"library"),
        size: 7,
    };
    take_member(&path, &member, &store).unwrap();
    assert_eq!(store.get(&member.sha256).unwrap().unwrap(), b"library");
    let absent = Member {
        path: "runtimes/y/lib.so".into(),
        ..member.clone()
    };
    assert!(
        take_member(&path, &absent, &store)
            .unwrap_err()
            .message
            .contains("no runtimes/y")
    );
    let resized = Member {
        size: 8,
        ..member.clone()
    };
    assert!(
        take_member(&path, &resized, &store)
            .unwrap_err()
            .message
            .contains("bytes")
    );
    let other = Member {
        path: "other".into(),
        sha256: Sha256::of(b"y"),
        size: 1,
    };
    assert_eq!(
        take_member(&path, &other, &store).unwrap_err().kind,
        Kind::Invalid
    );
    fs::write(root.join("junk"), b"not a zip").unwrap();
    assert_eq!(
        take_member(&root.join("junk"), &member, &store)
            .unwrap_err()
            .kind,
        Kind::Invalid
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn lists_exported_files_and_keeps_what_is_present() {
    let root = scratch("models");
    let store = ModelStore::new(&root);
    let lock = ModelLock::builtin();
    let segment = lock.model("segment.video").unwrap();
    let encoder = segment.graphs["image-encoder"].sha256;
    // A file the store has is not fetched again, whatever its kind.
    let fake = store.files().path(&encoder);
    fs::create_dir_all(store.files().root()).unwrap();
    fs::write(&fake, b"stand-in").unwrap();
    let fetcher = Fetcher::new(&store, |_| {});
    let done = fetcher.models(&lock, &["segment.video".into()]).unwrap();
    assert!(
        done.contains(&Fetched::Present {
            name: "segment.video image-encoder".into()
        }),
        "{done:?}"
    );
    assert!(
        done.contains(&Fetched::Exported {
            name: "segment.video memory-attention".into()
        }),
        "{done:?}"
    );
    assert_eq!(
        fetcher.models(&lock, &["absent".into()]).unwrap_err().kind,
        Kind::UnknownName
    );
    fs::remove_dir_all(&root).unwrap();
}

/// Downloads the runtime for this platform from its vendor and checks it:
/// 160 MB from the network.
#[test]
#[ignore = "reaches the network"]
fn fetches_the_runtime() {
    let root = scratch("runtime");
    let store = ModelStore::new(&root);
    let lock = ModelLock::builtin();
    let fetched = Fetcher::new(&store, |line| eprintln!("{line}"))
        .runtime(&lock)
        .unwrap();
    assert!(matches!(fetched, Fetched::Kept { .. }), "{fetched:?}");
    assert!(store.runtime_library(&lock).is_ok());
    fs::remove_dir_all(&root).unwrap();
}
