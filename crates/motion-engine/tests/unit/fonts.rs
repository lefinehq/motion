//! Unit tests for `src/fonts.rs`: the faces a scene asks for, scenes that
//! name a lock, and the judge that holds candidate files against stored
//! text.

use std::fs;
use std::path::PathBuf;

use motion_fonts::{Cache, Licence, Lock, LockedFont, Origin, Score, Sha256};
use motion_import::{ComposedLine, ComposedSegment};
use motion_scene::set_text;
use pretty_assertions::assert_eq;

use super::*;
use crate::{Kind, Session};

/// The raster crate's fixture font, an OFL subset of Lato Black whose
/// PostScript name is `FixtureSans-Black`.
const FIXTURE: &[u8] =
    include_bytes!("../../../motion-raster/tests/fixtures/FixtureSans-Black.ttf");

const SCENE: &str = r#"{"motion":1,"dur":"1s","fonts":["scene.fonts.lock"],
"comps":{"card":{"size":"200x100","dur":"1s","layers":[{"id":"label","text":"Label","font":"InterVariable 20"}]}},
"layers":[{"id":"title","text":"AB\nBA","font":"FixtureSans-Black 40"},{"id":"card","comp":"card"}]}"#;

/// A fresh folder for `name`.
fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("motion-engine-fonts-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn locked(bytes: &[u8]) -> LockedFont {
    LockedFont {
        postscript: "FixtureSans-Black".into(),
        family: "FixtureSans".into(),
        version: "Version 1.000".into(),
        licence: Licence::Ofl,
        source: Origin {
            repository: "owner/fixture".into(),
            reference: "1.0".into(),
            path: "FixtureSans-Black.ttf".into(),
        },
        sha256: Sha256::of(bytes),
        size: bytes.len() as u64,
    }
}

/// `SCENE` in a fresh folder with its lock beside it, and a cache that has
/// the locked file when `cached` is set.
fn locked_scene(name: &str, cached: bool) -> (Session, Cache) {
    let dir = scratch(name);
    fs::write(dir.join("scene.json"), SCENE).unwrap();
    Lock::new(vec![locked(FIXTURE)])
        .write(&lock_path(&dir.join("scene.json")))
        .unwrap();
    let cache = Cache::new(dir.join("cache"));
    if cached {
        cache.put(FIXTURE).unwrap();
    }
    let mut session = Session::open(dir.join("scene.json")).unwrap();
    session.set_font_cache(cache.clone());
    (session, cache)
}

#[test]
fn lists_the_fonts_a_scene_asks_for() {
    let session: Session = SCENE.parse().unwrap();
    assert_eq!(
        font_names(session.scene()),
        ["FixtureSans-Black", "InterVariable"]
    );
}

#[test]
fn a_lock_sets_text_from_the_cache() {
    let (session, _) = locked_scene("cached", true);
    assert!(
        session
            .fonts()
            .unwrap()
            .faces()
            .iter()
            .any(|f| f.postscript_name == "FixtureSans-Black")
    );
    assert!(session.graph().is_ok());
    assert_eq!(session.locks().len(), 1);
    let unlocked = session.unlocked_fonts().unwrap();
    assert!(
        !unlocked
            .faces()
            .iter()
            .any(|f| f.postscript_name == "FixtureSans-Black")
    );
}

#[test]
fn a_lock_whose_font_is_not_cached_names_the_fetch() {
    let (session, _) = locked_scene("uncached", false);
    let error = session.graph().unwrap_err();
    assert_eq!(error.kind, Kind::Missing);
    assert!(
        error.message.contains("motion fonts fetch"),
        "{}",
        error.message
    );
    assert_eq!(error.path.as_deref(), Some("fonts[0]"));
}

#[test]
fn a_lock_whose_font_has_changed_is_refused() {
    let (session, cache) = locked_scene("changed", true);
    fs::write(cache.font_path(&Sha256::of(FIXTURE)), b"not the font").unwrap();
    assert!(session.graph().unwrap_err().message.contains("has changed"));
}

#[test]
fn adds_a_fonts_entry_once_before_the_layers() {
    let mut scene: Value = serde_json::from_str(r#"{"motion":1,"dur":"1s","layers":[]}"#).unwrap();
    add_fonts_entry(&mut scene, "story.fonts.lock");
    add_fonts_entry(&mut scene, "story.fonts.lock");
    assert_eq!(
        scene.to_string(),
        r#"{"motion":1,"dur":"1s","fonts":["story.fonts.lock"],"layers":[]}"#
    );
    add_fonts_entry(&mut scene, "fonts/lato");
    assert_eq!(
        scene["fonts"],
        serde_json::json!(["story.fonts.lock", "fonts/lato"])
    );
}

#[test]
fn a_lock_is_named_after_its_scene() {
    assert_eq!(
        lock_path(Path::new("work/story.json")),
        Path::new("work/story.fonts.lock")
    );
}

#[test]
fn lists_the_fonts_the_set_lacks() {
    let session: Session = SCENE.parse().unwrap();
    assert_eq!(
        missing_fonts(session.scene(), &FontSet::builtin()),
        ["FixtureSans-Black"]
    );
}

/// The title as the fixture font sets it, stored as a project stores it:
/// one glyph per character, the paragraph's end a space.
fn stored_title(scene: &Scene) -> StoredText {
    let text = StoredText {
        comp: None,
        layer: "title".into(),
        lines: Vec::new(),
    };
    let mut fonts = FontSet::builtin();
    fonts.add_bytes(FIXTURE.to_vec(), "fixture").unwrap();
    let node = set_text(
        content_of(scene, &text).unwrap(),
        &mut Typesetter::new(fonts),
    )
    .unwrap();
    let layout = &node.layout;
    let lines = layout
        .lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let mut glyphs: Vec<u32> = layout
                .runs
                .iter()
                .filter(|run| run.line == i)
                .flat_map(|run| run.glyphs.iter().map(|g| g.id))
                .collect();
            glyphs.push(3);
            ComposedLine {
                origin: [0.0, line.baseline],
                baseline: line.baseline,
                segments: vec![ComposedSegment {
                    characters: line.characters + 1,
                    origin: [line.x, 0.0],
                    advances: Vec::new(),
                    width: line.width,
                    glyphs,
                }],
            }
        })
        .collect();
    StoredText { lines, ..text }
}

#[test]
fn the_judge_finds_the_file_a_layout_was_set_in() {
    let session: Session = SCENE.parse().unwrap();
    let scene = session.scene();
    let texts = [stored_title(scene)];
    let mut judge = LayoutJudge::new(scene, &texts, FontSet::builtin());
    let wanted = ["FixtureSans-Black".to_owned()];

    let right = judge
        .judge(&[FontFile {
            name: "fixture",
            bytes: FIXTURE,
        }])
        .unwrap();
    assert_eq!(right[0].fonts, wanted);
    let score = Score::of(&right, &wanted);
    assert!(score.is_exact(), "{score:?}");
    assert_eq!((score.lines.glyphs, score.lines.lines), (4, 2));

    // Another version: the same characters, other glyph ids.
    let mut moved = texts.clone();
    moved[0].lines[0].segments[0].glyphs[0] += 1;
    let mut judge = LayoutJudge::new(scene, &moved, FontSet::builtin());
    let other = Score::of(
        &judge
            .judge(&[FontFile {
                name: "fixture",
                bytes: FIXTURE,
            }])
            .unwrap(),
        &wanted,
    );
    assert_eq!(other.lines.glyphs_differ, 1);
    assert!(!other.is_exact());

    // No candidate has the face: the text is unset, and ranks last.
    let none = Score::of(&judge.judge(&[]).unwrap(), &wanted);
    assert_eq!(none.unset, 1);
    assert!(other.rank(&none).is_lt());
}
