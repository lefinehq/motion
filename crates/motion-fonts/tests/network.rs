//! The fetcher against the real sources. Ignored: it needs the network, and
//! uses a handful of GitHub's 60 hourly requests. Run it with
//! `cargo nextest run -p motion-fonts --features fetch --run-ignored only`.

#![cfg(feature = "fetch")]

use motion_fonts::fetch::{Fetcher, Request, Sources};
use motion_fonts::{Cache, Licence};

/// A scratch cache for `name`.
fn cache(name: &str) -> Cache {
    let root = std::env::temp_dir().join(format!(
        "motion-fonts-network-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    Cache::new(root)
}

#[test]
#[ignore = "needs the network"]
fn pins_a_release_of_a_family_repository() {
    let cache = cache("release");
    let mut fetcher = Fetcher::new(cache.clone(), Sources::builtin());
    let resolution = fetcher
        .resolve(Request {
            fonts: vec!["Montserrat-ExtraBold".into()],
            judge: None,
            release: Some("v3.100".into()),
        })
        .unwrap();
    assert!(
        resolution.unresolved.is_empty(),
        "{:?}",
        resolution.unresolved
    );
    let font = resolution.lock.get("Montserrat-ExtraBold").unwrap();
    assert_eq!(font.source.repository, "JulietaUla/Montserrat");
    assert_eq!(font.source.reference, "v3.100");
    assert_eq!(font.licence, Licence::Ofl);
    assert_eq!(
        font.sha256.to_string(),
        "46ad03e76f6c012434cba6a8b8f3bcef390f8b33da886cefd107fcbe2793b4c6"
    );
    assert_eq!(cache.load(font).unwrap().len() as u64, font.size);

    // A lock installs into an empty cache from its origins alone.
    let empty = self::cache("install");
    let mut fetcher = Fetcher::new(empty.clone(), Sources::builtin());
    fetcher.install(&resolution.lock).unwrap();
    assert_eq!(fetcher.downloaded(), 1);
    assert!(empty.load(font).is_ok());
}

#[test]
#[ignore = "needs the network"]
fn reports_fonts_no_open_source_has() {
    let mut fetcher = Fetcher::new(cache("licensed"), Sources::builtin());
    let resolution = fetcher
        .resolve(Request {
            fonts: vec!["Helvetica".into()],
            judge: None,
            release: None,
        })
        .unwrap();
    assert!(resolution.lock.fonts.is_empty());
    assert_eq!(resolution.unresolved[0].postscript, "Helvetica");
}
