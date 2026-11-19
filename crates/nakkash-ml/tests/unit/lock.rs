use super::*;

/// A lock of one model with one graph, its licence `class`.
pub(crate) fn small_lock(graph: &Sha256, constant: &Sha256, class: &str, obtain: &str) -> String {
    format!(
        r#"{{"lock":1,
  "runtime":{{"version":"1.31.0","licence":{{"spdx":"MIT","url":"u","class":"ok"}},
    "package":{{"url":"p","sha256":"{zero}","sha512":"x","size":1}},
    "libraries":{{"{here}":{{"path":"lib","sha256":"{zero}","size":1}}}}}},
  "models":{{"test.model":{{"does":"tests","licence":{{"spdx":"X","url":"u","class":"{class}"}},
    "graphs":{{"g":{{"sha256":"{graph}","size":24,"placement":"cpu","obtain":{obtain}}}}},
    "constants":{{"c":{{"sha256":"{constant}","size":136,"shape":[2],"obtain":{obtain}}}}}}}}},
  "refused":{{"test.refused":"non-commercial"}}}}"#,
        zero = Sha256::of(b""),
        here = platform(),
    )
}

pub(crate) const DOWNLOAD: &str = r#"{"download":{"url":"https://example.com/g.onnx"}}"#;

#[test]
fn the_builtin_lock_reads_and_has_every_role() {
    let lock = ModelLock::builtin();
    for key in ["segment.video", "matte.edges", "points.video", "flow.dense"] {
        assert!(lock.model(key).is_ok(), "{key}");
    }
    assert!(lock.runtime.libraries.contains_key("macos-aarch64"));
    assert!(lock.runtime.libraries.contains_key("linux-x86_64"));
    assert!(lock.files().iter().all(|f| f.size > 0));
}

#[test]
fn every_graph_the_lock_places_on_coreml_takes_fixed_shapes() {
    // D81: only fixed-shape graphs go to CoreML; these are the ones measured.
    let lock = ModelLock::builtin();
    let mut coreml: Vec<String> = lock
        .models
        .iter()
        .flat_map(|(k, m)| {
            m.graphs
                .iter()
                .filter(|(_, g)| g.placement == Placement::CoreMl)
                .map(move |(n, _)| format!("{k} {n}"))
        })
        .collect();
    coreml.sort();
    assert_eq!(
        coreml,
        [
            "flow.dense flow",
            "flow.dense flow-fast",
            "points.video features",
            "segment.video image-encoder",
            "segment.video memory-encoder"
        ]
    );
}

#[test]
fn reads_a_lock_and_finds_its_files() {
    let (g, c) = (Sha256::of(b"g"), Sha256::of(b"c"));
    let lock = ModelLock::read(&small_lock(&g, &c, "ok", DOWNLOAD)).unwrap();
    assert_eq!(lock.files().len(), 2);
    let pinned = lock.pinned(&g).unwrap();
    assert!(pinned.graph && pinned.part == "g" && pinned.name == "test.model");
    assert!(!lock.pinned(&c).unwrap().graph);
    assert!(lock.pinned(&Sha256::of(b"other")).is_none());
    assert!(lock.library_here().is_ok());
    assert_eq!(lock.model("absent").unwrap_err().kind, Kind::UnknownName);
}

#[rstest::rstest]
#[case::non_commercial("non-commercial", Kind::Conflict)]
#[case::copyleft("copyleft", Kind::Conflict)]
#[case::conditions_without_notice("conditions", Kind::Missing)]
fn refuses_a_licence_that_does_not_allow_commercial_use(#[case] class: &str, #[case] kind: Kind) {
    let text = small_lock(&Sha256::of(b"g"), &Sha256::of(b"c"), class, DOWNLOAD);
    let error = ModelLock::read(&text).unwrap_err();
    assert_eq!(error.kind, kind);
    assert!(error.message.contains("test.model"), "{}", error.message);
}

#[test]
fn refuses_a_refused_model_and_a_model_without_graphs() {
    let text = small_lock(&Sha256::of(b"g"), &Sha256::of(b"c"), "ok", DOWNLOAD);
    let refused = text.replace("\"test.refused\"", "\"test.model\"");
    assert_eq!(ModelLock::read(&refused).unwrap_err().kind, Kind::Conflict);
    let mut lock = ModelLock::read(&text).unwrap();
    lock.models.get_mut("test.model").unwrap().graphs.clear();
    let empty = serde_json::to_string(&lock).unwrap();
    assert_eq!(ModelLock::read(&empty).unwrap_err().kind, Kind::Invalid);
    assert_eq!(ModelLock::read("{}").unwrap_err().kind, Kind::Invalid);
    let version = text.replacen("\"lock\":1", "\"lock\":2", 1);
    assert_eq!(ModelLock::read(&version).unwrap_err().kind, Kind::Invalid);
}
