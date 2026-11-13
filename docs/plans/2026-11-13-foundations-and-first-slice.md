# Foundations and first slice: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a local `motion` repository that a newcomer enjoys reading, plus the first working slice. The slice reads a minimal scene, evaluates it at any frame, runs `check` (off-screen and outside-safe-area), applies one-line edits, and exposes all of this through a `motion` command-line tool. Every part is covered by detailed unit tests.

**Architecture:**
- A Cargo workspace of small crates, one per pipeline step: `motion-format` (read and validate) → `motion-scene` (compile) → `motion-eval` (values at frame t) → `motion-query` (checks) and `motion-edit` (edit language) → `motion-cli`.
- Edits change the JSON document itself, so what the author wrote is preserved. The document is then re-validated.
- Text size comes from an approximate measurer behind a trait until real shaping lands in M2.
- Later crates (raster, composite, encode, import, variants) are described in `ARCHITECTURE.md` but not created.

**Tech stack:**
- Rust 1.91, edition 2024
- `serde_json` 1.x with `preserve_order`
- `clap` 4.x with `derive`
- No other dependencies.

**Spec:** these files in `docs/design/`:
- `scene-format.md`
- `edits-and-checks.md`
- `quality.md` (craft lint, safe area)
- `decisions.md` (D4, D5, D6, D9, D18, D19)
- `scope.md`

## Global constraints

- **Toolchain:** `rust-toolchain.toml` pins `channel = "1.91"`. Every crate uses `edition = "2024"`.
- **Licence:** `MIT OR Apache-2.0`, with files `LICENSE-MIT` and `LICENSE-APACHE` (D9).
- **Language and naming:** English only. No third-party product names or trademarks in code, identifiers, docs or examples (D18). No internal product names or links to private repositories (D19).
- **Lints:**
  - `unsafe_code = "forbid"` and `missing_docs = "warn"` in `[workspace.lints.rust]`
  - `clippy::all = "warn"` in `[workspace.lints.clippy]`
  - every crate has `[lints] workspace = true`
- **Documentation and comments** (user requirement):
  - every crate's `lib.rs` and every module starts with a `//!` doc saying what it is for and where it sits in the pipeline;
  - every public item has a `///` doc;
  - non-obvious logic gets a short comment saying why.
- **Tests and checks:** every task's tests ship in the same commit as its code. Before every commit, all three of these must pass:
  ```
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```
- **Commits:**
  - signed (`git commit -S`; signing is already configured on this machine)
  - Conventional Commits, in English
  - no AI attribution trailers
  - no pushing
- **Dependencies:** only `serde_json` (feature `preserve_order`) and `clap` (feature `derive`). CLI integration tests use `std::process::Command` with `env!("CARGO_BIN_EXE_motion")`, not extra crates.
- **Number formatting:** everything printed or written uses `motion_format::fmt_num`. It rounds to 2 decimals, trims trailing zeros, and prints `-0` as `0`.
- **Units:** time is frames internally, as `f64`. Coordinates are pixels, and `at` positions a layer's **centre**.

## Review focus

The spec implies these inputs but no happy-path test exercises them. Each one is pinned by a test in the task named.

1. **A scene with an empty `layers` array:** `check` prints `OK 0` and `at` prints nothing (Task 6).
2. **A behaviour with duration `0f`:** jumps to its final state at its start frame, with no division by zero (Task 5).
3. **`at` with a time past the end:** returns final values. A negative time is an error (Task 6).
4. **Non-ASCII text (Arabic, emoji):** measured by characters, not bytes (Task 4).
5. **An edit value with spaces but no quotes** (`card at=center 666`): a clear error telling the user to quote the value (Task 7).

---

### Task 1: Workspace, repository hygiene, and units

**Files:**
- **Repository and toolchain:**
  - Create: `.gitignore` (`/target`), `rust-toolchain.toml`, `rustfmt.toml` (`edition = "2024"`), `Cargo.toml` (workspace).
- **Root documents:**
  - Create: `README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `CHANGELOG.md`, `LICENSE-MIT`, `LICENSE-APACHE`.
- **GitHub templates and CI:**
  - Create: `.github/workflows/ci.yml`, `.github/pull_request_template.md`, `.github/ISSUE_TEMPLATE/bug_report.md`, `.github/ISSUE_TEMPLATE/feature_request.md`.
- **The first crate:**
  - Create: `crates/motion-format/Cargo.toml`, `crates/motion-format/src/lib.rs`, `crates/motion-format/src/units.rs`, `crates/motion-format/src/number.rs`.

**Interfaces:**
- Produces, in `motion_format::units`:
  - `enum Time { Frames(f64), Seconds(f64) }`, with:
    - `Time::parse(&str) -> Result<Time, ShorthandError>`
    - `Time::to_frames(self, fps: f64) -> f64`
  - `struct Color { r: u8, g: u8, b: u8, a: u8 }`, with `Color::parse(&str) -> Result<Color, ShorthandError>`.
  - `struct Size { width: u32, height: u32 }`, with `Size::parse(&str) -> Result<Size, ShorthandError>`.
- Produces, in `motion_format`:
  - `struct ShorthandError { message: String, expected: &'static str }`, which implements `Display` as `"{message}; expected {expected}"`.
  - `fn fmt_num(f64) -> String`, defined in `number.rs` and re-exported from the crate root.

**Setup and root files:**

- [ ] **Step 1: Set up the repository.** In `/Users/fatihdevs/Desktop/Projects/better-i18n/motion`, run `git init -b main`. Then write:
  - the workspace `Cargo.toml`, with `members = ["crates/*"]`, `resolver = "3"`, the `[workspace.package]` fields (`version = "0.0.0"`, `edition = "2024"`, `license = "MIT OR Apache-2.0"`, `rust-version = "1.91"`, `repository = "https://github.com/lefinehq/motion"`), the lint tables from Global constraints, and `[workspace.dependencies]` with `serde_json = { version = "1", features = ["preserve_order"] }` and `clap = { version = "4", features = ["derive"] }`;
  - the toolchain, rustfmt and gitignore files.

- [ ] **Step 2: Write the root documents.**
  - **`README.md`:**
    - one sentence on what motion is;
    - status ("early development, nothing to install yet");
    - links into `docs/README.md`;
    - a "Building from source" section (`cargo test --workspace`);
    - a licence section.

    It makes no performance claims: published numbers must come from `bench/`.
  - **`ARCHITECTURE.md`:**
    - the pipeline diagram from `docs/design/rendering.md`;
    - a table of the crates, one line each on what they do and what they must never do (for example "`motion-format` never touches pixels");
    - the planned crates (`motion-raster`, `motion-composite`, `motion-encode`, `motion-import`, `motion-variants`), marked "not yet created";
    - a "where to start reading" path: `motion-format/src/lib.rs` → `motion-eval/src/evaluate.rs`.
  - **`CONTRIBUTING.md`:**
    - the three pre-commit commands;
    - the test-with-every-change rule;
    - the doc-comment rule;
    - Conventional Commits and signed commits;
    - the naming rule (D18);
    - the doc-writing rules (link to `docs/README.md#how-we-write-docs`).
  - **`CODE_OF_CONDUCT.md`:** adopts Contributor Covenant 2.1 by link, with reports through the contact listed on the `lefinehq` GitHub organisation profile.
  - **`SECURITY.md`:** reports go through GitHub private vulnerability reporting on the repository.
  - **`CHANGELOG.md`:** Keep a Changelog format, with an `## [Unreleased]` section.
  - **Licence files:** standard MIT and Apache-2.0 texts, copyright "2026 The motion authors".
  - **GitHub templates:** short PR and issue templates that ask for tests and a doc update.
  - **`ci.yml`:** runs on push and pull request. Its steps: checkout, install the toolchain from `rust-toolchain.toml`, then `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. It runs on `ubuntu-latest` and `macos-latest`.

**Units and number formatting:**

- [ ] **Step 3: Write the failing tests.** They go in `units.rs` and `number.rs`, inside `#[cfg(test)] mod tests`:

```rust
#[test] fn time_parses_frames_and_seconds() {
    assert_eq!(Time::parse("12f").unwrap(), Time::Frames(12.0));
    assert_eq!(Time::parse("0.4s").unwrap(), Time::Seconds(0.4));
    assert_eq!(Time::parse("5s").unwrap().to_frames(30.0), 150.0);
    assert_eq!(Time::parse("12f").unwrap().to_frames(60.0), 12.0);
}
#[test] fn time_rejects_bad_input_with_expected_form() {
    let e = Time::parse("12").unwrap_err();
    assert!(e.to_string().contains("expected `<number>f` or `<number>s`, e.g. `12f` or `0.4s`"));
    assert!(Time::parse("-3f").is_err());
    assert!(Time::parse("f").is_err());
}
#[test] fn color_parses_three_six_and_eight_digits() {
    assert_eq!(Color::parse("#FFF").unwrap(), Color { r: 255, g: 255, b: 255, a: 255 });
    assert_eq!(Color::parse("#0B0D12").unwrap(), Color { r: 11, g: 13, b: 18, a: 255 });
    assert_eq!(Color::parse("#00000033").unwrap().a, 0x33);
    assert!(Color::parse("red").unwrap_err().to_string().contains("`#RGB`, `#RRGGBB` or `#RRGGBBAA`"));
    assert!(Color::parse("#GGG").is_err());
}
#[test] fn size_parses_presets_and_explicit_pixels() {
    assert_eq!(Size::parse("1080p").unwrap(), Size { width: 1920, height: 1080 });
    assert_eq!(Size::parse("720p").unwrap(), Size { width: 1280, height: 720 });
    assert_eq!(Size::parse("4k").unwrap(), Size { width: 3840, height: 2160 });
    assert_eq!(Size::parse("square").unwrap(), Size { width: 1080, height: 1080 });
    assert_eq!(Size::parse("vertical").unwrap(), Size { width: 1080, height: 1920 });
    assert_eq!(Size::parse("640x360").unwrap(), Size { width: 640, height: 360 });
    assert!(Size::parse("0x360").is_err());
    assert!(Size::parse("big").unwrap_err().to_string().contains("`720p`, `1080p`, `4k`, `square`, `vertical` or `<width>x<height>`"));
}
#[test] fn fmt_num_rounds_and_trims() {
    assert_eq!(fmt_num(960.0), "960");
    assert_eq!(fmt_num(0.5), "0.5");
    assert_eq!(fmt_num(1.234), "1.23");
    assert_eq!(fmt_num(-0.001), "0");
    assert_eq!(fmt_num(580.8), "580.8");
}
```

- [ ] **Step 4: Run the tests and confirm they fail.** Run `cargo test -p motion-format`. Expected: compile errors, because nothing is defined yet.

- [ ] **Step 5: Implement `units.rs`, `number.rs` and the `lib.rs` re-exports.**
  - **Times:** the number must be finite and ≥ 0. A unit suffix is required.
  - **Colours:** `#RGB` doubles each digit.
  - **Sizes:** a `WxH` size needs both sides > 0.

- [ ] **Step 6: Run the tests and confirm they pass.** Run `cargo test -p motion-format`. Expected: 5 passed. Then run the fmt and clippy commands from Global constraints, and expect them to be clean.

- [ ] **Step 7: Commit.**
  ```
  git add -A
  git commit -S -m "chore: set up workspace, repository docs and CI; add format units"
  ```
  This commit also includes the existing `docs/`.

---

### Task 2: Shorthand parsers

**Files:**
- Create: `crates/motion-format/src/shorthand/mod.rs` (re-exports), `position.rs`, `rect.rs`, `font.rs`, `behaviour.rs`
- Modify: `crates/motion-format/src/lib.rs` (`pub mod shorthand;`)

**Interfaces:**
- Consumes: `Time`, `ShorthandError`, `fmt_num` (Task 1).
- Produces, in `motion_format::shorthand`:
  - `enum Coord { Center, Px(f64) }`.
  - `struct Position { x: Coord, y: Coord }`, with `Position::parse` and `impl Display`. Display prints `center` or a number through `fmt_num`, with the two tokens separated by a space.
  - `struct RectShape { width: f64, height: f64, radius: f64 }`, with `RectShape::parse`.
  - `struct Font { family: String, size: f64, weight: u16 }`, with `Font::parse`.
  - `enum Preset { Fade, FadeUp, FadeDown, FadeLeft, FadeRight }`.
  - `enum Easing { Linear, EaseIn, EaseOut, EaseInOut }`.
  - `struct Behaviour { preset: Preset, distance: f64, duration: Time, easing: Easing, start: Time }`, with `Behaviour::parse`.
  - Every `parse` takes `&str` and returns `Result<_, ShorthandError>`.

The exact `expected` strings are copied into the tests below.

- [ ] **Step 1: Write the failing tests.** One `mod tests` per file:

```rust
// position.rs
#[test] fn position_parses_center_and_pixels() {
    let p = Position::parse("center 260").unwrap();
    assert_eq!((p.x, p.y), (Coord::Center, Coord::Px(260.0)));
    assert_eq!(Position::parse("100 -20").unwrap().y, Coord::Px(-20.0));
    assert_eq!(Position::parse("center 260").unwrap().to_string(), "center 260");
    assert!(Position::parse("260").unwrap_err().to_string()
        .contains("`<x> <y>`, each a number of pixels or `center`, e.g. `center 260`"));
    assert!(Position::parse("left 3").is_err());
}
// rect.rs
#[test] fn rect_parses_size_and_optional_radius() {
    assert_eq!(RectShape::parse("300x400 r24").unwrap(), RectShape { width: 300.0, height: 400.0, radius: 24.0 });
    assert_eq!(RectShape::parse("240x8").unwrap().radius, 0.0);
    assert!(RectShape::parse("300").unwrap_err().to_string().contains("`<width>x<height> [r<radius>]`, e.g. `300x400 r24`"));
    assert!(RectShape::parse("0x10").is_err());
}
// font.rs
#[test] fn font_parses_family_with_spaces_size_and_weight() {
    assert_eq!(Font::parse("Inter 96 700").unwrap(), Font { family: "Inter".into(), size: 96.0, weight: 700 });
    assert_eq!(Font::parse("Noto Sans Arabic 40").unwrap(), Font { family: "Noto Sans Arabic".into(), size: 40.0, weight: 400 });
    assert!(Font::parse("Inter").unwrap_err().to_string().contains("`<family> <size> [<weight>]`, e.g. `Inter 96 700`"));
    assert!(Font::parse("Inter 96 950").is_err()); // weight must be 100..=900
}
// behaviour.rs
#[test] fn behaviour_uses_defaults() {
    let b = Behaviour::parse("fade-up").unwrap();
    assert_eq!(b, Behaviour { preset: Preset::FadeUp, distance: 40.0, duration: Time::Frames(12.0), easing: Easing::EaseOut, start: Time::Frames(0.0) });
}
#[test] fn behaviour_reads_every_token_in_any_order() {
    let b = Behaviour::parse("fade-up 60px 12f ease-out @15f").unwrap();
    assert_eq!((b.distance, b.duration, b.start), (60.0, Time::Frames(12.0), Time::Frames(15.0)));
    assert_eq!(Behaviour::parse("fade @0.5s linear 0.4s").unwrap().duration, Time::Seconds(0.4));
}
#[test] fn behaviour_rejects_unknown_and_repeated_tokens() {
    let e = Behaviour::parse("fade-upp").unwrap_err().to_string();
    assert!(e.contains("unknown preset `fade-upp`"));
    assert!(e.contains("`<preset> [<n>px] [<n>f|<n>s] [<easing>] [@<time>]`, e.g. `fade-up 40px 12f ease-out @0f`"));
    assert!(Behaviour::parse("fade 12f 10f").unwrap_err().to_string().contains("duration given twice"));
    assert!(Behaviour::parse("fade bounce").unwrap_err().to_string().contains("unknown token `bounce`"));
}
```

- [ ] **Step 2: Run the tests and confirm they fail.** Run `cargo test -p motion-format shorthand`. Expected: compile errors.

- [ ] **Step 3: Implement the four parsers.**
  - **Behaviour tokens** are classified by shape, never by position, so every string has exactly one parse:
    - the first token is the preset;
    - `*px` is the distance;
    - `@*` is the start;
    - `*f` or `*s` is the duration;
    - `linear`, `ease-in`, `ease-out` and `ease-in-out` are easings.
  - **The preset names** are `fade`, `fade-up`, `fade-down`, `fade-left` and `fade-right`.
  - **A comment** in `behaviour.rs` must explain why classification is by shape.

- [ ] **Step 4: Run the tests and confirm they pass.** Run `cargo test -p motion-format`. Expected: all pass. fmt and clippy must be clean.

- [ ] **Step 5: Commit.**
  ```
  git commit -S -m "feat(format): add position, rect, font and behaviour shorthands"
  ```

---

### Task 3: Scene parsing, validation, and the canonical writer

**Files:**
- Create: `crates/motion-format/src/error.rs`, `scene.rs`, `parse.rs`, `write.rs`
- Create: `examples/launch.json`
- Test: unit tests in each file, plus `crates/motion-format/tests/example_scene.rs`

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces, in `motion_format`:
  - `struct FormatError { path: String, message: String, expected: Option<String> }`. Its `Display` is `"{path}: {message}"`, followed by `"; expected {e}"` when `expected` is set. Paths look like `layers[1].in`, or `scene` for the top level.
  - `struct Scene { version: u32, size: Size, fps: f64, duration: Time, background: Color, layers: Vec<Layer> }`.
  - `struct Layer { id: String, content: Content, fill: Color, at: Position, enter: Option<Behaviour> }`. The `enter` field is JSON key `in`.
  - `enum Content { Text { text: String, font: Font }, Rect(RectShape) }`.
  - `struct Document { raw: serde_json::Value, scene: Scene }`.
  - Functions:
    - `parse_document(json: &str) -> Result<Document, FormatError>`
    - `scene_from_value(&Value) -> Result<Scene, FormatError>`
    - `write_document(&Value) -> String`
  - Constants:
    - `SCENE_KEYS = ["motion","size","fps","dur","bg","layers"]`
    - `LAYER_KEYS = ["id","text","rect","font","fill","at","in"]`
  - Defaults:
    - `fps` 30
    - `bg` `#000000`
    - `fill` `#FFFFFF`
    - `at` `center center`
    - `font` `Inter 48 400`
  - `motion` must be `1`. `dur` and `layers` are required.

`examples/launch.json`, exactly, ending with a newline:
```
{"motion":1,"size":"1080p","fps":30,"dur":"5s","bg":"#0B0D12",
"layers":[
{"id":"title","text":"Launch Week","font":"Inter 96 700","at":"center 260","in":"fade-up 12f"},
{"id":"card","rect":"300x400 r24","fill":"#FFFFFF","at":"center 700","in":"fade-up 60px 12f ease-out @15f"}
]}
```

- [ ] **Step 1: Write the failing tests.**

```rust
// parse.rs
#[test] fn parses_minimal_scene_with_defaults() {
    let d = parse_document(r#"{"motion":1,"dur":"2s","layers":[{"id":"a","rect":"10x10"}]}"#).unwrap();
    assert_eq!(d.scene.fps, 30.0);
    assert_eq!(d.scene.size, Size { width: 1920, height: 1080 });
    assert_eq!(d.scene.layers[0].fill, Color { r: 255, g: 255, b: 255, a: 255 });
    assert_eq!(d.scene.layers[0].at, Position { x: Coord::Center, y: Coord::Center });
    assert!(d.scene.layers[0].enter.is_none());
}
#[test] fn errors_carry_a_path_and_the_expected_form() {
    let e = parse_document(r#"{"motion":1,"dur":"2s","layers":[{"id":"a","rect":"10x10"},{"id":"b","rect":"10x10","in":"zoom"}]}"#).unwrap_err();
    assert_eq!(e.path, "layers[1].in");
    assert!(e.to_string().starts_with("layers[1].in: unknown preset `zoom`"));
}
#[test] fn rejects_structural_mistakes() {
    let err = |j: &str| parse_document(j).unwrap_err().to_string();
    assert!(err(r#"{"motion":2,"dur":"1s","layers":[]}"#).contains("unsupported version 2"));
    assert!(err(r#"{"motion":1,"layers":[]}"#).contains("scene: missing `dur`"));
    assert!(err(r#"{"motion":1,"dur":"1s","layers":[],"colour":"#FFF"}"#).contains("unknown key `colour`"));
    assert!(err(r#"{"motion":1,"dur":"1s","layers":[{"id":"a"}]}"#).contains("needs exactly one of `text` or `rect`"));
    assert!(err(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1"},{"id":"a","rect":"1x1"}]}"#).contains("duplicate id `a`"));
    assert!(err(r#"{"motion":1,"dur":"1s","layers":[{"id":"Bad Id","rect":"1x1"}]}"#).contains("ids use lowercase letters, digits, `-` and `_`, starting with a letter"));
    assert!(err(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1","font":"Inter 10"}]}"#).contains("`font` only applies to text layers"));
    assert!(err("not json").starts_with("scene: invalid JSON"));
}
// write.rs
#[test] fn writes_one_layer_per_line_with_layers_last() {
    let v: serde_json::Value = serde_json::from_str(r#"{"layers":[{"id":"a","rect":"1x1"}],"motion":1,"dur":"1s"}"#).unwrap();
    assert_eq!(write_document(&v), "{\"motion\":1,\"dur\":\"1s\",\n\"layers\":[\n{\"id\":\"a\",\"rect\":\"1x1\"}\n]}\n");
    let empty: serde_json::Value = serde_json::from_str(r#"{"motion":1,"dur":"1s","layers":[]}"#).unwrap();
    assert_eq!(write_document(&empty), "{\"motion\":1,\"dur\":\"1s\",\n\"layers\":[\n]}\n");
}
// tests/example_scene.rs
#[test] fn example_scene_parses_and_round_trips_byte_for_byte() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/launch.json")).unwrap();
    let doc = motion_format::parse_document(&src).unwrap();
    assert_eq!(doc.scene.layers.len(), 2);
    assert_eq!(motion_format::write_document(&doc.raw), src);
}
```

- [ ] **Step 2: Run the tests and confirm they fail.** Run `cargo test -p motion-format`. Expected: compile errors.

- [ ] **Step 3: Implement the parser and the writer.**
  - **Parser.** Convert `serde_json::Value` into `Scene` by hand, not with serde derive, so that every error carries its path. Keys are strict: an unknown key is an error that lists the allowed keys. Ids must match `[a-z][a-z0-9_-]*`.
  - **Writer.** It prints `"layers"` last, whatever order the keys came in:
    - every other key is minified on the first line, which ends with `,` and a newline;
    - then `"layers":[`, with each layer minified on its own line, joined by `,\n`;
    - then `]}` and a newline.

- [ ] **Step 4: Run the tests and confirm they pass.** Run `cargo test -p motion-format`. Expected: all pass. fmt and clippy must be clean.

- [ ] **Step 5: Commit.**
  ```
  git commit -S -m "feat(format): parse and validate scenes, write canonical documents"
  ```

---

### Task 4: Compile scenes into a scene graph

**Files:**
- Create: `crates/motion-scene/Cargo.toml` (depends on `motion-format`), `src/lib.rs`, `src/graph.rs`, `src/compile.rs`, `src/text.rs`, `src/easing.rs`

**Interfaces:**
- Consumes: `Scene`, `Layer`, `Content`, `Font`, `Coord`, `Behaviour`, `Preset`, `Easing`, `Time` (Tasks 1–3).
- Produces, in `motion_scene`:
  - `struct SceneGraph { width: f64, height: f64, fps: f64, duration_frames: f64, nodes: Vec<Node> }`.
  - `enum NodeKind { Text, Rect }`.
  - `struct Node { id: String, kind: NodeKind, x: f64, y: f64, width: f64, height: f64, enter: Option<Enter> }`, where `x` and `y` are the resolved centre.
  - `struct Enter { start: f64, duration: f64, easing: Easing, from_offset: (f64, f64) }`, with times in frames.
  - `trait TextMeasure { fn measure(&self, text: &str, font: &Font) -> (f64, f64); }`.
  - `struct ApproxTextMeasure`: width = characters × 0.55 × size, height = 1.2 × size. Its doc must say it is a placeholder until real shaping (M2).
  - `fn compile(scene: &Scene, measure: &dyn TextMeasure) -> SceneGraph`.
  - `fn easing::sample(e: Easing, p: f64) -> f64`.

- [ ] **Step 1: Write the failing tests.**

```rust
// compile.rs
fn launch() -> motion_format::Scene { motion_format::parse_document(include_str!("../../../examples/launch.json")).unwrap().scene }
#[test] fn resolves_centre_positions_and_sizes() {
    let g = compile(&launch(), &ApproxTextMeasure);
    assert_eq!((g.width, g.height, g.duration_frames), (1920.0, 1080.0, 150.0));
    let t = &g.nodes[0];
    assert_eq!((t.x, t.y), (960.0, 260.0));
    assert!((t.width - 580.8).abs() < 1e-9 && (t.height - 115.2).abs() < 1e-9);
    let c = &g.nodes[1];
    assert_eq!((c.x, c.y, c.width, c.height), (960.0, 700.0, 300.0, 400.0));
}
#[test] fn converts_behaviours_to_frame_based_entrances() {
    let g = compile(&launch(), &ApproxTextMeasure);
    let e = g.nodes[1].enter.as_ref().unwrap();
    assert_eq!((e.start, e.duration, e.from_offset), (15.0, 12.0, (0.0, 60.0)));
}
#[test] fn offsets_point_away_from_the_direction_of_travel() {
    // fade-up starts below (+y), fade-down above (-y), fade-left right of (+x), fade-right left of (-x)
    for (preset, off) in [("fade-up", (0.0, 40.0)), ("fade-down", (0.0, -40.0)), ("fade-left", (40.0, 0.0)), ("fade-right", (-40.0, 0.0)), ("fade", (0.0, 0.0))] {
        let j = format!(r#"{{"motion":1,"dur":"1s","layers":[{{"id":"a","rect":"10x10","in":"{preset}"}}]}}"#);
        let s = motion_format::parse_document(&j).unwrap().scene;
        assert_eq!(compile(&s, &ApproxTextMeasure).nodes[0].enter.as_ref().unwrap().from_offset, off, "{preset}");
    }
}
// text.rs
#[test] fn measures_characters_not_bytes() {
    let f = motion_format::shorthand::Font::parse("Inter 100").unwrap();
    assert_eq!(ApproxTextMeasure.measure("مرحبا", &f).0, 5.0 * 0.55 * 100.0);
    assert_eq!(ApproxTextMeasure.measure("🎬🎬", &f).0, 2.0 * 0.55 * 100.0);
}
// easing.rs
#[test] fn easings_hit_endpoints_and_known_midpoints() {
    for e in [Easing::Linear, Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut] {
        assert!(sample(e, 0.0).abs() < 1e-9 && (sample(e, 1.0) - 1.0).abs() < 1e-9);
    }
    assert_eq!(sample(Easing::Linear, 0.25), 0.25);
    assert!((sample(Easing::EaseOut, 0.5) - 0.684).abs() < 1e-3);
    assert!((sample(Easing::EaseInOut, 0.5) - 0.5).abs() < 1e-6);
    assert!((0..=100).map(|i| sample(Easing::EaseIn, i as f64 / 100.0)).collect::<Vec<_>>().windows(2).all(|w| w[1] >= w[0]));
    assert_eq!(sample(Easing::EaseOut, -1.0), 0.0); // clamps
}
```

- [ ] **Step 2: Run the tests and confirm they fail.** Run `cargo test -p motion-scene`. Expected: compile errors.

- [ ] **Step 3: Implement.**
  - **Easing curves** are cubic béziers with these control points:
    - ease-in `(0.42, 0, 1, 1)`
    - ease-out `(0, 0, 0.58, 1)`
    - ease-in-out `(0.42, 0, 0.58, 1)`
  - **Solving.** For x(t) = p, use bisection for 40 iterations; it is monotonic on [0, 1] for these points. Add a comment saying why bisection is used: it is robust, and speed doesn't matter yet.
  - **Clamping.** `p` is clamped to [0, 1].

- [ ] **Step 4: Run the tests and confirm they pass.** Run `cargo test -p motion-scene`. Expected: all pass, and the workspace checks are clean.

- [ ] **Step 5: Commit.**
  ```
  git commit -S -m "feat(scene): compile scenes into a scene graph with eased entrances"
  ```

---

### Task 5: Evaluate a frame

**Files:**
- Create: `crates/motion-eval/Cargo.toml`, `src/lib.rs`, `src/state.rs`, `src/evaluate.rs`

**Interfaces:**
- Consumes: `SceneGraph`, `Node`, `Enter`, `easing::sample` (Task 4).
- Produces, in `motion_eval`:
  - `struct NodeState { id: String, x: f64, y: f64, width: f64, height: f64, opacity: f64 }`, with `left()`, `right()`, `top()` and `bottom()`.
  - `struct FrameState { frame: f64, nodes: Vec<NodeState> }`.
  - `fn evaluate(graph: &SceneGraph, frame: f64) -> FrameState`.

- [ ] **Step 1: Write the failing tests.**

```rust
fn graph(json: &str) -> motion_scene::SceneGraph {
    motion_scene::compile(&motion_format::parse_document(json).unwrap().scene, &motion_scene::ApproxTextMeasure)
}
const LIN: &str = r#"{"motion":1,"dur":"2s","layers":[{"id":"c","rect":"100x100","at":"center 700","in":"fade-up 60px 12f linear @15f"}]}"#;
#[test] fn hidden_and_offset_before_the_entrance() {
    let n = &evaluate(&graph(LIN), 0.0).nodes[0];
    assert_eq!((n.opacity, n.y), (0.0, 760.0));
}
#[test] fn interpolates_during_the_entrance() {
    let n = &evaluate(&graph(LIN), 21.0).nodes[0];
    assert_eq!((n.opacity, n.y), (0.5, 730.0));
    assert_eq!((n.top(), n.bottom(), n.left(), n.right()), (680.0, 780.0, 910.0, 1010.0));
}
#[test] fn settles_at_the_end_and_stays_there() {
    for f in [27.0, 59.0, 1000.0] { let n = &evaluate(&graph(LIN), f).nodes[0]; assert_eq!((n.opacity, n.y), (1.0, 700.0)); }
}
#[test] fn layers_without_entrance_are_always_visible() {
    let n = &evaluate(&graph(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"10x10"}]}"#), 0.0).nodes[0];
    assert_eq!(n.opacity, 1.0);
}
#[test] fn zero_duration_jumps_at_its_start_frame() {
    let g = graph(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"10x10","in":"fade 0f @5f"}]}"#);
    assert_eq!(evaluate(&g, 4.9).nodes[0].opacity, 0.0);
    assert_eq!(evaluate(&g, 5.0).nodes[0].opacity, 1.0);
}
#[test] fn sub_frame_times_are_supported() {
    assert!((evaluate(&graph(LIN), 15.6).nodes[0].opacity - 0.05).abs() < 1e-9);
}
```

- [ ] **Step 2: Run the tests and confirm they fail.** Run `cargo test -p motion-eval`. Expected: compile errors.

- [ ] **Step 3: Implement `evaluate`.**
  - Before `start`: progress 0, so opacity is 0 and the full offset applies.
  - Otherwise: p = (frame − start) ÷ duration. When duration is 0, p is 1.
  - The eased value `k` sets opacity = k and position = base + offset × (1 − k).
  - Document the meaning of the `frame` argument: sub-frame times are allowed, for motion blur later.

- [ ] **Step 4: Run the tests and confirm they pass.** Run `cargo test -p motion-eval`. Expected: all pass, and the workspace checks are clean.

- [ ] **Step 5: Commit.**
  ```
  git commit -S -m "feat(eval): evaluate scene graphs at any frame"
  ```

---

### Task 6: `check` and `at` queries

**Files:**
- Create: `crates/motion-query/Cargo.toml`, `src/lib.rs`, `src/report.rs`, `src/check.rs`, `src/rules/mod.rs`, `src/rules/bounds.rs`, `src/at.rs`

**Interfaces:**
- Consumes: `Scene`, `Position`, `Coord`, `fmt_num` (format); `SceneGraph` (scene); `evaluate`, `NodeState` (eval).
- Produces, in `motion_query`:
  - `enum Severity { Error, Warning }`, which displays as `E` / `W`.
  - `struct Violation { severity: Severity, code: &'static str, id: String, first_frame: u32, last_frame: u32, amount_px: f64, fix: Option<String> }`. It implements `Display` as `"{sev} {code} {id} {first}-{last}f +{amount}px"`, followed by `"; fix: {fix}"` when a fix exists.
  - `struct Report { violations: Vec<Violation>, passed: usize }`. Its `Display` prints the violation lines, then `OK {passed}`, joined by `\n`. It also has `has_errors()`.
  - `fn check(scene: &Scene, graph: &SceneGraph) -> Report`.
  - `fn at(graph: &SceneGraph, frame: f64, ids: &[String]) -> Result<String, QueryError>`.
  - `struct QueryError { message: String }`, which implements `Display`.

**Rules:**
- **`offscreen` (E):** a visible node's box (opacity > 0) extends past the canvas.
- **`unsafe` (W):** the box extends past the title-safe area, which is the central 90% (an inset of 5% of the width and 5% of the height on each side). This rule is skipped for any node that has an `offscreen` violation, to keep the report short.

**How frames are checked:**
- Sample integer frames from `0` to `ceil(duration_frames) - 1`.
- The amount on a frame is the largest overshoot across the four edges.
- Consecutive violating frames merge into one line. The amount shown is the line's maximum, rounded up to a whole pixel.
- `passed` counts the (rule, node) pairs that were evaluated and had no violation.

**Fix hints:**
- Shift the node's `at` along the axis of the largest overshoot by the rounded amount: subtract for the right or bottom edge, add for the left or top edge.
- Resolve `center` to a number only on the axis that changes.
- Format: `{id} at="{x} {y}"`.

**`at` output:**
- One line per requested id, or per node when the list is empty, in scene order.
- Format: `{id} x={x} y={y} w={w} h={h} opacity={o}`, with numbers formatted by `fmt_num`.
- An unknown id gives `no layer with id \`{id}\``.
- A negative frame gives `time must be 0 or later`.

- [ ] **Step 1: Write the failing tests.**

```rust
fn run(json: &str) -> String {
    let s = motion_format::parse_document(json).unwrap().scene;
    let g = motion_scene::compile(&s, &motion_scene::ApproxTextMeasure);
    check(&s, &g).to_string()
}
#[test] fn clean_scene_reports_ok_count() {
    assert_eq!(run(include_str!("../../../examples/launch.json")), "OK 4");
}
#[test] fn offscreen_reports_range_amount_and_fix() {
    assert_eq!(run(r#"{"motion":1,"dur":"5s","layers":[{"id":"card","rect":"300x400","at":"center 1000"}]}"#),
        "E offscreen card 0-149f +120px; fix: card at=\"center 880\"\nOK 0");
}
#[test] fn unsafe_area_is_a_warning() {
    assert_eq!(run(r#"{"motion":1,"dur":"5s","layers":[{"id":"card","rect":"300x100","at":"center 1010"}]}"#),
        "W unsafe card 0-149f +34px; fix: card at=\"center 976\"\nOK 1");
}
#[test] fn entrance_overshoot_is_caught_only_while_visible() {
    assert_eq!(run(r#"{"motion":1,"dur":"1s","layers":[{"id":"card","rect":"300x100","at":"center 1000","in":"fade-up 60px 12f linear @0f"}]}"#),
        "E offscreen card 1-5f +25px; fix: card at=\"center 975\"\nOK 0");
}
#[test] fn left_edge_fix_adds_and_resolves_only_the_moving_axis() {
    assert_eq!(run(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"200x100","at":"50 center"}]}"#),
        "E offscreen a 0-29f +50px; fix: a at=\"100 center\"\nOK 0");
}
#[test] fn empty_scene_is_ok_zero() {
    assert_eq!(run(r#"{"motion":1,"dur":"1s","layers":[]}"#), "OK 0");
}
// at.rs
#[test] fn at_prints_resolved_values() {
    let s = motion_format::parse_document(include_str!("../../../examples/launch.json")).unwrap().scene;
    let g = motion_scene::compile(&s, &motion_scene::ApproxTextMeasure);
    assert_eq!(at(&g, 200.0, &["title".into()]).unwrap(), "title x=960 y=260 w=580.8 h=115.2 opacity=1");
    assert_eq!(at(&g, 0.0, &[]).unwrap().lines().count(), 2);
    assert_eq!(at(&g, 0.0, &["nope".into()]).unwrap_err().to_string(), "no layer with id `nope`");
    assert_eq!(at(&g, -1.0, &[]).unwrap_err().to_string(), "time must be 0 or later");
    let empty = motion_scene::compile(&motion_format::parse_document(r#"{"motion":1,"dur":"1s","layers":[]}"#).unwrap().scene, &motion_scene::ApproxTextMeasure);
    assert_eq!(at(&empty, 0.0, &[]).unwrap(), "");
}
```

- [ ] **Step 2: Run the tests and confirm they fail.** Run `cargo test -p motion-query`. Expected: compile errors.

- [ ] **Step 3: Implement the module.** Put the rules in `rules/bounds.rs`, frame sampling and range merging in `check.rs`, and formatting in `report.rs`. Add a module comment in `rules/mod.rs` describing how to add a new rule.

- [ ] **Step 4: Run the tests and confirm they pass.** Run `cargo test -p motion-query`. Expected: all pass, and the workspace checks are clean.

- [ ] **Step 5: Commit.**
  ```
  git commit -S -m "feat(query): add check with offscreen and safe-area rules, and at"
  ```

---

### Task 7: The edit language

**Files:**
- Create: `crates/motion-edit/Cargo.toml`, `src/lib.rs`, `src/parse.rs`, `src/apply.rs`, `src/error.rs`

**Interfaces:**
- Consumes: `scene_from_value`, `SCENE_KEYS`, `LAYER_KEYS`, `FormatError` (format).
- Produces, in `motion_edit`:
  - `struct Edit { target: String, changes: Vec<Change> }`.
  - `struct Change { key: String, value: EditValue }`.
  - `enum EditValue { Set(serde_json::Value), Remove }`.
  - `fn parse_edit(line: &str) -> Result<Edit, EditError>`.
  - `fn apply_edit(raw: &serde_json::Value, edit: &Edit) -> Result<serde_json::Value, EditError>`.
  - `struct EditError { message: String }`, which implements `Display`.

**Grammar:**
- An edit is `<target> <key>=<value> [<key>=<value> ...]`.
- The target is a layer id, or `scene` for the top level.
- A value is one of:
  - a quoted string, with `\"` and `\\` escapes;
  - `none`, which removes the key;
  - an integer or float, which becomes a number;
  - `true` or `false`;
  - any other bare word, which becomes a string.
- Only `=` is supported in this slice.

**Applying an edit:**
- The edit works on a clone of `raw`.
- The result is checked with `scene_from_value`. Format errors are passed through as `invalid result: {format error}`.

- [ ] **Step 1: Write the failing tests.**

```rust
// parse.rs
#[test] fn parses_targets_keys_and_typed_values() {
    let e = parse_edit(r#"title text="Launch \"Week\"" fill=#FFF fps=60 in=none"#).unwrap();
    assert_eq!(e.target, "title");
    assert_eq!(e.changes[0].value, EditValue::Set(serde_json::json!("Launch \"Week\"")));
    assert_eq!(e.changes[1].value, EditValue::Set(serde_json::json!("#FFF")));
    assert_eq!(e.changes[2].value, EditValue::Set(serde_json::json!(60)));
    assert_eq!(e.changes[3].value, EditValue::Remove);
}
#[test] fn unquoted_spaces_get_a_helpful_error() {
    let m = parse_edit("card at=center 666").unwrap_err().to_string();
    assert!(m.contains("expected `<key>=<value>`, got `666`") && m.contains("quote values that contain spaces"));
}
#[test] fn rejects_unsupported_operators_and_empty_lines() {
    assert!(parse_edit("card delay+=10f").unwrap_err().to_string().contains("operator `+=` is not supported yet; use `=`"));
    assert!(parse_edit("   ").unwrap_err().to_string().contains("empty edit"));
    assert!(parse_edit("card").unwrap_err().to_string().contains("no changes"));
    assert!(parse_edit(r#"card text="open"#).unwrap_err().to_string().contains("unclosed quote"));
}
// apply.rs
fn doc() -> serde_json::Value { motion_format::parse_document(include_str!("../../../examples/launch.json")).unwrap().raw }
#[test] fn sets_and_removes_layer_keys() {
    let out = apply_edit(&doc(), &parse_edit(r#"card fill=#FF0000 in=none"#).unwrap()).unwrap();
    assert_eq!(out["layers"][1]["fill"], "#FF0000");
    assert!(out["layers"][1].get("in").is_none());
    assert_eq!(out["layers"][0], doc()["layers"][0]); // untouched layers stay byte-identical
}
#[test] fn edits_the_scene_itself() {
    assert_eq!(apply_edit(&doc(), &parse_edit("scene bg=#000000 fps=60").unwrap()).unwrap()["fps"], 60);
}
#[test] fn reports_unknown_targets_keys_and_invalid_results() {
    let err = |l: &str| apply_edit(&doc(), &parse_edit(l).unwrap()).unwrap_err().to_string();
    assert_eq!(err("ghost fill=#FFF"), "no layer with id `ghost`");
    assert!(err("card colour=#FFF").contains("unknown key `colour` for a layer; allowed: id, text, rect, font, fill, at, in"));
    assert!(err("card fill=blue").starts_with("invalid result: layers[1].fill"));
    assert!(err("card id=title").contains("duplicate id `title`"));
}
```

- [ ] **Step 2: Run the tests and confirm they fail.** Run `cargo test -p motion-edit`. Expected: compile errors.

- [ ] **Step 3: Implement the tokenizer and the applier.** The tokenizer is quote-aware. The applier validates every key against `LAYER_KEYS` or `SCENE_KEYS`.

- [ ] **Step 4: Run the tests and confirm they pass.** Run `cargo test -p motion-edit`. Expected: all pass, and the workspace checks are clean.

- [ ] **Step 5: Commit.**
  ```
  git commit -S -m "feat(edit): add one-line edit language for scenes"
  ```

---

### Task 8: The `motion` command-line tool

**Files:**
- Create: `crates/motion-cli/Cargo.toml` (`[[bin]] name = "motion"`), `src/main.rs`, `src/commands/mod.rs`, `src/commands/check.rs`, `src/commands/at.rs`, `src/commands/edit.rs`
- Create: `crates/motion-cli/tests/cli.rs`, `crates/motion-cli/tests/fixtures/offscreen.json` (the offscreen scene from Task 6, as a file)
- Modify: `README.md` (a "Try it" section), `CHANGELOG.md` (an `[Unreleased]` entry)

**Interfaces:**
- Consumes: every crate above.
- Produces these commands:
  - `motion check <scene>`
  - `motion at <scene> <time> [ids...]`, where the time is parsed by `Time::parse`
  - `motion edit <scene> <line>`, which writes the file through `write_document`, then prints the check report

**Exit codes:**
- `0` when there are no errors.
- `1` when the check report has an `E` line.
- `2` for unreadable files, format errors, edit errors and query errors. These print to stderr as `error: {message}`.

- [ ] **Step 1: Write the failing integration tests.** They go in `tests/cli.rs` and use `Command::new(env!("CARGO_BIN_EXE_motion"))`.

```rust
#[test] fn check_clean_example_exits_zero() { /* stdout == "OK 4\n", status 0, on examples/launch.json */ }
#[test] fn check_offscreen_exits_one() { /* stdout starts "E offscreen card 0-149f +120px", status 1 */ }
#[test] fn at_accepts_seconds_and_frames() { /* `at launch.json 1s title` == `at launch.json 30f title`; status 0 */ }
#[test] fn at_unknown_id_exits_two() { /* stderr == "error: no layer with id `nope`\n", status 2 */ }
#[test] fn edit_writes_canonical_file_and_prints_report() {
    /* copy fixtures/offscreen.json to a temp dir under env!("CARGO_TARGET_TMPDIR");
       run `edit <copy> 'card at="center 700"'`; status 0; stdout "OK 2\n";
       the file then parses, and its second line is "\"layers\":[" */
}
#[test] fn edit_invalid_value_leaves_file_untouched() { /* `fill=blue` → status 2, file bytes unchanged */ }
#[test] fn invalid_scene_exits_two_with_path() { /* scene with "in":"zoom" → stderr contains "layers[0].in: unknown preset `zoom`" */ }
```

Fill in each test's body, asserting exactly the values in its comment.

- [ ] **Step 2: Run the tests and confirm they fail.** Run `cargo test -p motion-cli`. Expected: compile errors, because there is no binary yet.

- [ ] **Step 3: Implement the tool.**
  - `main.rs` is a `clap` derive enum dispatching into `commands/*`.
  - Each command returns `Result<ExitCode, String>`.
  - The error message goes to stderr in `main.rs` only.
  - Add doc comments on every subcommand; clap shows them as `--help`.
- [ ] **Step 4: Update the docs.**
  - Add a "Try it" section to `README.md` with the three commands run against `examples/launch.json`.
  - Add an entry under `CHANGELOG.md` `[Unreleased]`: "First slice: scene format, check, at, edit."

- [ ] **Step 5: Run every check and confirm it passes.** Run the three Global-constraint commands. Expected: everything passes, with no warnings.

- [ ] **Step 6: Commit.**
  ```
  git commit -S -m "feat(cli): add motion check, at and edit commands"
  ```
