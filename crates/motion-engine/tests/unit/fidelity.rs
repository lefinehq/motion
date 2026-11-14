//! Unit tests for `src/fidelity.rs`: a manifest read, a sample measured
//! against its own frames, and ledgers compared.

use pretty_assertions::assert_eq;

use super::*;

/// A fixture project with one star, which the import does not support.
const STAR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../motion-import/tests/fixtures/Star.aep"
);

/// A scratch folder for the test `name`, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("motion-fidelity-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create a scratch folder");
    dir
}

/// A measure with every error at `error` and no text.
fn measure_at(error: f64) -> Measure {
    let window = Window {
        from: 0.0,
        frames: 3,
        content_error: Some(error),
        content_failing: Some(error),
        drawn_error: error,
        drawn_failing: error,
        union_error: error,
        union_failing: error,
    };
    Measure {
        unsupported: BTreeMap::from([("stars".to_owned(), 1)]),
        texts: 0,
        texts_within: 0,
        text_worst: Some(0.0),
        all: window.clone(),
        settled: Some(Window {
            from: 1.0,
            ..window
        }),
    }
}

#[test]
fn a_manifest_reads_paths_from_its_own_folder() {
    let dir = scratch("manifest");
    let path = dir.join("samples.json");
    fs::write(
        &path,
        r#"{"samples":[{"id":"s1","project":"p/a.aep","fonts":["fonts"],"reference":"/frames","fps":25,"ignore":["bg"]}]}"#,
    )
    .unwrap();
    let manifest = Manifest::read(&path).unwrap();
    let sample = &manifest.samples[0];
    assert_eq!(sample.project, dir.join("p/a.aep"));
    assert_eq!(sample.fonts, [dir.join("fonts")]);
    assert_eq!(sample.reference, PathBuf::from("/frames"));
    assert_eq!((sample.start, sample.settled), (0.0, None));
    fs::write(&path, r#"{"samples":[{"id":"s1","name":"x"}]}"#).unwrap();
    let error = Manifest::read(&path).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert!(
        error.message.contains("unknown field `name`"),
        "{}",
        error.message
    );
}

#[test]
fn a_sample_measured_against_its_own_frames_has_no_error() {
    let dir = scratch("own");
    let work = dir.join("work");
    let first = Sample {
        id: "star".into(),
        project: STAR.into(),
        comp: None,
        fonts: Vec::new(),
        reference: dir.join("reference"),
        fps: 30.0,
        start: 0.0,
        ignore: Vec::new(),
        settled: Some(1.0),
    };
    // The reference: the imported scene itself, drawn every second.
    let import = import_scene(STAR, &Options::default(), None).unwrap();
    let session: Session = write_document(&import.json).parse().unwrap();
    fs::create_dir_all(&first.reference).unwrap();
    for (i, frame) in [0.0, 60.0, 120.0].iter().enumerate() {
        let png = session.render(*frame).unwrap().to_png().unwrap();
        fs::write(first.reference.join(format!("f{i}.png")), png).unwrap();
    }
    let sample = Sample { fps: 0.5, ..first };
    let measure = measure(&sample, &work, true).unwrap();
    assert_eq!(
        measure.unsupported.values().sum::<usize>(),
        1,
        "{measure:?}"
    );
    assert_eq!((measure.texts, measure.text_worst), (0, Some(0.0)));
    assert_eq!(measure.all.frames, 3);
    assert_eq!(measure.all.union_failing, 0.0);
    assert_eq!(measure.all.content_failing, Some(0.0));
    assert_eq!(measure.settled.as_ref().map(|w| w.frames), Some(2));
    assert!(work.join("star/scene.json").exists());
    assert!(work.join("star/errors/f0.png").exists());
}

#[test]
fn comparing_ledgers_names_what_got_better_or_worse() {
    let before = Ledger {
        samples: BTreeMap::from([
            ("a".to_owned(), measure_at(0.1)),
            ("gone".to_owned(), measure_at(0.1)),
        ]),
    };
    let mut better = measure_at(0.05);
    better.all.drawn_error = 0.1 + NOISE / 2.0;
    better.texts_within = 1;
    better.unsupported.clear();
    let after = Ledger {
        samples: BTreeMap::from([("a".to_owned(), better)]),
    };
    let changes = compare(&before, &after);
    let of = |metric: &str| {
        changes
            .iter()
            .find(|c| c.sample == "a" && c.metric == metric)
            .map(|c| (c.before, c.after, c.better))
    };
    assert_eq!(of("all drawn_error"), None, "within the noise");
    assert_eq!(
        of("settled union_failing"),
        Some((Some(0.1), Some(0.05), Some(true)))
    );
    assert_eq!(
        of("settled content_failing"),
        Some((Some(0.1), Some(0.05), Some(true)))
    );
    assert_eq!(of("texts_within"), Some((Some(0.0), Some(1.0), Some(true))));
    assert_eq!(of("unsupported: stars"), Some((Some(1.0), None, None)));
    assert!(
        changes
            .iter()
            .any(|c| c.sample == "gone" && c.after.is_none())
    );
    let worse = compare(&after, &before);
    assert!(
        worse
            .iter()
            .any(|c| c.metric == "settled content_failing" && c.better == Some(false))
    );
    // A ledger from before the content window: its numbers are new, not worse.
    let mut old = measure_at(0.1);
    old.settled = None;
    old.all.content_error = None;
    old.all.content_failing = None;
    let older = Ledger {
        samples: BTreeMap::from([("a".to_owned(), old)]),
    };
    let first = compare(&older, &before);
    let content = first
        .iter()
        .find(|c| c.metric == "all content_failing")
        .unwrap();
    assert_eq!((content.before, content.better), (None, None));
}

#[test]
fn a_ledger_round_trips_rounded() {
    let dir = scratch("ledger");
    let path = dir.join("ledger.json");
    assert_eq!(Ledger::read(&path).unwrap(), Ledger::default());
    let mut measure = measure_at(0.123_456_789);
    measure.text_worst = None;
    let ledger = Ledger {
        samples: BTreeMap::from([("a".to_owned(), measure)]),
    };
    fs::write(&path, ledger.to_json()).unwrap();
    let read = Ledger::read(&path).unwrap();
    assert_eq!(read.samples["a"].all.drawn_error, 0.123_457);
    assert_eq!(read.samples["a"].text_worst, None);
    assert!(ledger.to_json().ends_with("}\n"));
    // A ledger written before the content window still reads.
    let old: String = ledger
        .to_json()
        .lines()
        .filter(|line| !line.contains("\"content_"))
        .collect();
    fs::write(&path, old).unwrap();
    assert_eq!(
        Ledger::read(&path).unwrap().samples["a"]
            .all
            .content_failing,
        None
    );
}
