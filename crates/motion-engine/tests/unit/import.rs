//! Unit tests for `src/import.rs`.

use tracing_subscriber::layer::SubscriberExt;

use super::*;
use crate::Kind;
use crate::telemetry::timing::Timings;
use crate::test_support::with_subscriber;

/// A chunk with `tag` and `body` (of even length).
fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let size = u32::try_from(body.len()).expect("a small body");
    [tag.as_slice(), &size.to_be_bytes(), body].concat()
}

/// The smallest project: a file version and an empty root folder.
fn empty_project() -> Vec<u8> {
    let head = chunk(
        b"head",
        &[0, 94, 0, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    );
    let fold = chunk(b"LIST", b"Fold");
    chunk(b"RIFX", &[b"Egg!".as_slice(), &head, &fold].concat())
}

/// A file holding `bytes` in a fresh directory of its own.
fn scratch_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("motion-engine-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("project.aep");
    fs::write(&path, bytes).expect("write project");
    path
}

#[test]
fn reads_a_project_in_the_read_and_decode_steps() {
    let path = scratch_file("import", &empty_project());
    let timings = Timings::default();
    let project = with_subscriber(tracing_subscriber::registry().with(timings.layer()), || {
        import(&path)
    })
    .expect("a project");
    assert_eq!(project.version.to_string(), "94.9");
    assert!(project.items.is_empty());
    let steps: Vec<String> = timings.steps().into_iter().map(|(name, _)| name).collect();
    assert_eq!(steps, ["read", "decode"]);
}

#[test]
fn reports_a_missing_file_and_a_file_that_is_not_a_project() {
    let missing = import("/no/such/project.aep").unwrap_err();
    assert_eq!(missing.kind, Kind::Io);
    let path = scratch_file("not-a-project", b"{\"motion\":1}");
    let error = import(&path).unwrap_err();
    assert_eq!(
        (error.kind, error.expected.as_deref()),
        (
            Kind::Invalid,
            Some("a file that starts with `RIFX` and `Egg!`")
        )
    );
}
