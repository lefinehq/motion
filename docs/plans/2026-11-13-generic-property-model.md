# Generic property model: implementation plan

**Goal:** replace the first slice's hard-coded animation logic with one generic model. In that model, anything a designer can animate is expressible, and adding a preset, a curve or a property is a data change, not a new branch in the code.

**Why:**
- In the first slice, every preset was a `match` arm, there were only four named easings, and the evaluator knew exactly one animation: the entrance. That cannot express a master project, and every new preset would mean new code.
- See decision D27, added in Task 7.

**Architecture:**
- **Curves.** Every easing is a `Curve`: `linear`, `hold` or a cubic bézier. Named curves (`ease-out`, …) live in a table.
- **Properties.** Every animatable property is a row in a registry: name, default and allowed range. In this plan the rows are `x`, `y`, `opacity`, `scale` and `rotate`. Adding a property means adding a row, plus its effect on geometry if it has one.
- **Tracks.** Every property of every node is a `Track`: keyframes, each with the curve that leads to the next key. A static value is a one-key track. Evaluation samples tracks generically and knows nothing about presets.
- **Presets are data.** A preset is a JSON entry that gives each property's starting value relative to its resting value, e.g. `"fade-up": {"opacity": 0, "y": "+d"}`. The built-in presets are in `presets.json`, embedded in the crate. A scene can add or override presets under `"presets"`.
- **`anim`.** Layers can animate any registered property directly, either with a two-key string (`"0 -> 1 @0-12f ease-out"`) or with a keyframe array (`[["0f",0],["12f",1,"cubic(.2,0,0,1)"]]`).
- **Edits.** Edits can address nested keys (`card anim.opacity=...`) and take JSON arrays and objects as values.

**Tech stack:**
- Rust 1.91, edition 2024
- `serde_json` with `preserve_order`
- `clap`
- `thiserror` 2 for error types (Task 0)
- dev-only: `proptest` and `insta`
- CI tools: `cargo-llvm-cov` and `cargo-deny`

**Spec:**
- `docs/design/scene-format.md`: the three animation levels (behaviours, property tracks, keyframes)
- `docs/design/edits-and-checks.md`
- `docs/design/decisions.md`

## Global constraints

- **Toolchain, licence, lints, naming:** as in the first plan.
  - edition 2024
  - `MIT OR Apache-2.0`
  - `[lints] workspace = true`
  - English only
  - no third-party product names or trademarks (D18) and no internal names (D19)
- **Documentation and comments** (user requirement):
  - every module starts with a `//!` doc saying what it is for and where it sits in the pipeline;
  - every public item has a `///` doc;
  - non-obvious logic gets a short comment saying why.
- **No hard-coded lists in code** (user requirement):
  - Presets, named curves and property names are data: tables or embedded JSON, looked up by name.
  - A `match` is fine for a closed set of genuinely different behaviours, such as the three `Curve` variants or the content kinds. It is not acceptable for listing names that are really data.
  - Errors about an unknown name list the allowed names, taken from the same table.
- **Dependencies:** only those listed under Tech stack. Every error type derives `thiserror::Error`; no hand-written `Display` for errors. Every parser of user text gets a `proptest` test showing it never panics.
- **Errors** name what was wrong and show the expected form (`; expected …`), because an agent must be able to fix its input in one try.
- **Where tests live** (user requirement): no test code in `src/`. Unit tests live in a separate folder that mirrors `src/`: the tests for `crates/<c>/src/<path>.rs` are in `crates/<c>/tests/unit/<path>.rs`. They are attached as a child module, so they can still reach private items: the last line of the source file is `#[cfg(test)] #[path = "<relative path>"] mod tests;`, where the path is relative to the source file's directory (for example `../tests/unit/curve.rs` from `src/curve.rs`). There is no `tests/unit/main.rs`, so Cargo does not build the folder as an integration target. Integration tests that use only the public API (such as the CLI tests) stay directly in `tests/`. `include_str!` paths are relative to the test file.
- **Tests and checks:** every task's tests ship in the same commit as its code. Before every commit, all three of these must pass:
  ```
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```
- **Commits:**
  - signed (`git commit -S`)
  - Conventional Commits, in English
  - no attribution trailers of any kind
  - only your own paths
  - no pushing
- **Behaviour must not regress:** `examples/launch.json` still checks `OK 4`. Existing CLI tests keep passing unless a task says how their expected output changes.
- **Units:** time is frames as `f64` internally. Coordinates are pixels, `x` and `y` are the layer's centre, and `rotate` is in degrees, clockwise.

## Review focus

1. **Zero-length segments:** two keys on the same frame. The later key wins from that frame on (Task 4).
2. **`hold`:** the value stays at the left key until the next key's frame, then jumps (Tasks 1 and 4).
3. **Overshooting curves:** `opacity` is clamped by its range, while `x`, `y`, `scale` and `rotate` may overshoot (Task 5).
4. **The same property in `in` and in `anim`:** a format error that says to put the entrance in `anim` instead (Task 3).
5. **Rotated or scaled boxes:** checks use the axis-aligned bounding box of the transformed box (Task 5).
6. **A fix hint on an axis driven by `anim`:** no shift on that axis, because `at` does not move it (Task 5).

---

### Task 0: Tooling for contributors

**Files:**
- Modify:
  - `Cargo.toml`: `[workspace.dependencies]` gains `thiserror = "2"`, `proptest = "1"` and `insta = "1"`.
  - the error types in `motion-format` (`error.rs`), `motion-query` (`error.rs`) and `motion-edit` (`error.rs`)
  - `.github/workflows/ci.yml`
  - `CONTRIBUTING.md`: also explain where tests live.
  - every `src/**/*.rs` file that has a `mod tests` block
- Create:
  - `deny.toml`
  - `crates/*/tests/unit/**`
  - proptest tests next to each shorthand parser and the edit tokenizer

**What to do:**
- **Errors.** `ShorthandError`, `FormatError`, `QueryError` and `EditError` derive `thiserror::Error`, with `#[error(...)]` replacing the hand-written `Display` impls. Every message stays byte-identical, and the existing tests prove it.
- **Property tests.** `proptest` tests assert that `Time::parse`, `Color::parse`, `Size::parse`, `Position::parse`, `RectShape::parse`, `Font::parse`, `Behaviour::parse` and `parse_edit` never panic on arbitrary strings. Where a parser has a `Display`, `parse(display(x)) == x`.
- **CI.** Two new jobs:
  - **Coverage:** `cargo llvm-cov --workspace --fail-under-lines 85`, with the tool installed by `taiki-e/install-action`.
  - **Licences and advisories:** `cargo deny check`.
- **`deny.toml`.** Allowed licences: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0 and Zlib. Copyleft is not allowed (the codec licence policy). Advisories: deny vulnerabilities.
- **`CONTRIBUTING.md`.** A short "Tools" section: how to run coverage and `cargo deny` locally, and that `insta` is for snapshot tests of text output (`cargo insta review`).

- [ ] **Step 1:** Move every existing `#[cfg(test)] mod tests { … }` block out of `src/` into `tests/unit/`, following the "Where tests live" rule. This is a pure move: test bodies stay unchanged except for `include_str!` paths and `use` lines, and the test count must stay the same (report it before and after).
- [ ] **Step 2:** Convert the errors and add the property tests, the new tests also under `tests/unit/`. All existing tests must pass unchanged.
- [ ] **Step 3:** Add `deny.toml`, the CI jobs and the CONTRIBUTING section. If `cargo-deny` or `cargo-llvm-cov` is installed locally, run it. Otherwise say so in the report; CI will run them.
- [ ] **Step 4: Run all three checks, and commit in two parts:**
  1. `test: move unit tests into tests/unit` (the move only)
  2. `build: add thiserror, property tests, coverage and licence checks`

---

### Task 1: Curves as data

**Files:**
- Create: `crates/motion-format/src/curve.rs`.
- Modify:
  - `crates/motion-format/src/shorthand/behaviour.rs`: drop `Easing`; the behaviour's curve is a `Curve`.
  - `crates/motion-format/src/lib.rs`
  - `crates/motion-scene/src/easing.rs`: rename the module to `curve.rs`, with `sample(Curve, p)`.
  - `crates/motion-scene/src/compile.rs`
  - `crates/motion-scene/src/graph.rs`
  - `crates/motion-eval/src/evaluate.rs`

**Interfaces:**
- Produces, in `motion_format::curve`:
  - `enum Curve { Linear, Hold, Bezier(f64, f64, f64, f64) }`
  - `const NAMED_CURVES: &[(&str, Curve)]`, holding:
    - `linear`
    - `hold`
    - `ease` = (0.25, 0.1, 0.25, 1)
    - `ease-in` = (0.42, 0, 1, 1)
    - `ease-out` = (0, 0, 0.58, 1)
    - `ease-in-out` = (0.42, 0, 0.58, 1)
  - `Curve::parse(&str) -> Result<Curve, ShorthandError>`:
    - accepts a name from the table, or `cubic(x1,y1,x2,y2)`;
    - x1 and x2 must be in 0..=1;
    - an unknown name lists every name in `NAMED_CURVES` plus `cubic(x1,y1,x2,y2)`.
  - `Curve::is_curve_token(&str) -> bool`, which the behaviour tokenizer uses to classify a token.
  - `impl Display for Curve`: prints the table name when the curve matches a table entry exactly, otherwise `cubic(…)` with values formatted by `fmt_num`.
- Produces, in `motion_scene`: `fn sample(curve: Curve, p: f64) -> f64`.
  - `p` is clamped to 0..=1, and the endpoints are pinned.
  - `Hold` returns 0 for p < 1 and 1 at p = 1.
  - `Bezier` uses the existing bisection solver.

- [ ] **Step 1: Tests first.**
  - **Curve parsing:** every table name; `cubic` values; the x-range error; the error for an unknown name, which lists the table.
  - **Display:** a round trip; `cubic(0,0,0.58,1)` prints `ease-out`.
  - **Sampling:** `hold`; every named curve equals its `cubic` form.
  - Every existing behaviour, easing and evaluate test keeps passing after its types are renamed.
- [ ] **Step 2: Implement.** No `match` on curve names anywhere; names exist only in `NAMED_CURVES`.
- [ ] **Step 3: Run all three checks. Commit:** `refactor(format): make easing curves a data table with cubic and hold`.

---

### Task 2: Property registry and data-driven presets

**Files:**
- Create:
  - `crates/motion-format/src/property.rs`
  - `crates/motion-format/src/preset.rs`
  - `crates/motion-format/src/presets.json`
- Modify:
  - `crates/motion-format/src/shorthand/behaviour.rs`: drop the `Preset` enum; `preset: String`.
  - `crates/motion-format/src/parse.rs`: validate preset names; add the `presets` scene key.
  - `crates/motion-format/src/scene.rs`: `Scene.presets`.
  - `crates/motion-format/src/lib.rs`

**Interfaces:**
- Produces, in `motion_format::property`:
  - `struct PropertySpec { name: &'static str, default: f64, range: Option<(f64, f64)>, doc: &'static str }`
  - `const PROPERTIES: &[PropertySpec]`, in this order:

    | Name | Default | Range | Meaning |
    |---|---|---|---|
    | `x` | 0 | none | centre x in px; the resting value comes from `at` |
    | `y` | 0 | none | centre y in px; the resting value comes from `at` |
    | `opacity` | 1 | 0..=1 | |
    | `scale` | 1 | none | uniform factor; negative values mirror |
    | `rotate` | 0 | none | degrees, clockwise |

  - `fn property(name: &str) -> Option<(usize, &'static PropertySpec)>`
  - `fn property_names() -> String`, the comma-separated list used in error messages.
- Produces, in `motion_format::preset`:
  - `enum Start { Value(f64), Plus, Minus }`. `Plus` and `Minus` are the resting value plus or minus the behaviour's distance; JSON `"+d"` and `"-d"`.
  - `struct PresetSpec { props: Vec<(usize, Start)> }`, where the index points into `PROPERTIES`. Properties are kept in the order they are written.
  - `fn builtin_presets() -> &'static [(String, PresetSpec)]`.
    - It parses `presets.json` (via `include_str!`) once, using `std::sync::OnceLock`.
    - The file is checked by a unit test, so a bad built-in fails `cargo test`.
  - `fn parse_presets(value: &Value, path: &str) -> Result<Vec<(String, PresetSpec)>, FormatError>`, which also parses a scene's `presets`.
    - Preset names follow the id rule.
    - Properties must be in the registry.
    - A start value is a number or `"+d"`/`"-d"`; anything else is an error with the expected form.
  - `presets.json`, built-ins:

    ```json
    {"fade":{"opacity":0},
     "fade-up":{"opacity":0,"y":"+d"},
     "fade-down":{"opacity":0,"y":"-d"},
     "fade-left":{"opacity":0,"x":"+d"},
     "fade-right":{"opacity":0,"x":"-d"},
     "pop":{"opacity":0,"scale":0.6},
     "spin-in":{"opacity":0,"rotate":-90}}
    ```

  - `Scene.presets: Vec<(String, PresetSpec)>`. The resolved table is the scene's entries followed by the built-ins; a scene entry overrides a built-in with the same name.
  - `fn Scene::preset(&self, name: &str) -> Option<&PresetSpec>`.
  - `SCENE_KEYS` gains `presets`.
- `Behaviour.preset: String`. `parse.rs` checks it against the scene's resolved presets. An unknown name keeps the message `unknown preset \`zoom\``, adds `; expected one of …` listing the names, and uses the path `layers[i].in`.

- [ ] **Step 1: Tests first.**
  - The registry lookup.
  - The built-in file parses, and every property it names exists.
  - A scene preset adds a new name, and overrides a built-in.
  - The errors for:
    - a bad start value
    - an unknown property in a preset
    - an unknown preset name, whose message lists the names
  - The CLI test `invalid_scene_exits_two_with_path` still passes.
- [ ] **Step 2: Implement.** No `match` on preset names or property names.
- [ ] **Step 3: Run all three checks. Commit:** `refactor(format): define properties in a registry and presets as data`.

---

### Task 3: `anim`: any property, any keyframes

**Files:**
- Create: `crates/motion-format/src/anim.rs`.
- Modify:
  - `crates/motion-format/src/parse.rs`: `anim` layer key; conflict check.
  - `crates/motion-format/src/scene.rs`: `Layer.anim`.
  - `crates/motion-format/src/lib.rs`

**Interfaces:**
- Produces, in `motion_format::anim`:
  - `struct AnimKey { time: Time, value: f64, curve: Curve }`. The curve leads to the next key; the default is `linear`.
  - `struct AnimTrack { property: usize, keys: Vec<AnimKey> }`
  - `fn parse_anim(value: &Value, path: &str) -> Result<Vec<AnimTrack>, FormatError>`. It accepts an object mapping property names to tracks, in one of two forms:
    - **A string** of the form `"<from> -> <to> @<start>-<end>[<unit>] [<curve>]"`, for example `"0 -> 200 @0-30f ease-out"`, `"0 -> 1 @1-1.5s"` or `"0 -> 1 @0f-12f"`.
      - The unit after `<end>` also applies to `<start>` when `<start>` has none.
      - It produces two keys; the curve sits on the first.
    - **An array** of keys: `[time, value]` or `[time, value, curve]`.
      - `time` is a time string (`"12f"`, `"0.5s"`) or a bare number of frames.
      - Keys must be in non-decreasing time order, and there must be at least one.
  - **Validation:**
    - A property name must be in the registry; otherwise the error lists the allowed names.
    - Values must be finite and inside the property's range; for example, `opacity` 2 gives an error that shows the range.
    - Every error has a path such as `layers[1].anim.opacity[2]`.
- `Layer.anim: Vec<AnimTrack>`. `LAYER_KEYS` gains `anim` after `in`.
- **Conflict rule:** if `in`'s preset sets a property that `anim` also sets, the error at `layers[i].anim.<prop>` says the property is already animated by `in`, and to put the entrance in `anim` instead.

- [ ] **Step 1: Tests first.**
  - Both forms parse into identical keys.
  - Seconds are kept as seconds (`Time`).
  - The default curve is `linear`.
  - Every error above, with its path.
  - The conflict error.
  - The `edit` test that lists `LAYER_KEYS` is updated: `allowed: id, text, rect, font, fill, at, in, anim`.
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run all three checks. Commit:** `feat(format): animate any registered property with anim tracks and keyframes`.

---

### Task 4: Tracks in the scene graph

**Files:**
- Create: `crates/motion-scene/src/track.rs`.
- Modify:
  - `crates/motion-scene/src/graph.rs`: `Node` holds tracks; drop `Enter`.
  - `crates/motion-scene/src/compile.rs`
  - `crates/motion-scene/src/lib.rs`

**Interfaces:**
- Produces, in `motion_scene`:
  - `struct Key { frame: f64, value: f64, curve: Curve }`
  - `struct Track { keys: Vec<Key> }`. It is never empty and its keys are sorted. It has:
    - `Track::constant(v)`
    - `Track::sample(frame) -> f64`:
      - before the first key: the first value;
      - after the last key: the last value;
      - between two keys: the value eased with the left key's curve;
      - keys on the same frame: the later key wins from that frame on.
    - `Track::is_constant()`
  - `Node { id, kind, width, height, tracks: Vec<Track>, follows_at: (bool, bool) }`:
    - `tracks` is parallel to `PROPERTIES`.
    - `follows_at.0` is false when `x` is driven by `anim`; `follows_at.1` is the same for `y`.
- **How `compile` builds the tracks:**
  - **Resting values:** `x` and `y` come from `at`; every other property uses its registry default.
  - **`in`:** look up the preset (scene presets first, then built-ins). For each `(property, start)`, the track is `[Key{start_frame, from, curve}, Key{start_frame + duration, rest, Linear}]`. `from` is the number given, or rest ± distance.
  - **`anim`:** the keys convert to frames with the scene fps.
  - **Everything else:** `Track::constant(rest)`.
  - There is no `match` on property names. Rest values for `x` and `y` are looked up by name through the registry.

- [ ] **Step 1: Tests first.**
  - Track sampling: before, between and after keys; `hold`; same-frame keys; a one-key track.
  - Compile builds the same values the first slice produced for `fade-up` and friends.
  - Seconds-based `anim` at 60 fps.
  - A scene-defined preset.
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run all three checks. Commit:** `refactor(scene): compile every property into a keyframe track`.

---

### Task 5: Generic evaluation, transformed bounds and queries

**Files:**
- Modify:
  - `crates/motion-eval/src/state.rs`
  - `crates/motion-eval/src/evaluate.rs`
  - `crates/motion-query/src/rules/*.rs`
  - `crates/motion-query/src/check.rs`
  - `crates/motion-query/src/at.rs`
  - `crates/motion-cli/tests/cli.rs`
  - `README.md`

**Interfaces:**
- `NodeState { id, width, height, values: Vec<f64> }`. `values` is parallel to `PROPERTIES`, and each value is clamped to its property's range.
  - `fn get(&self, name: &str) -> f64` looks a value up through the registry, and panics only on a name not in the registry.
  - `fn bounds(&self) -> (f64, f64, f64, f64)` returns left, top, right, bottom: the axis-aligned bounding box of the box `width × |scale|` by `height × |scale|`, rotated by `rotate` around (`x`, `y`).
  - `left()`, `right()`, `top()` and `bottom()` read from `bounds()`.
- `evaluate` samples every track. It has no per-property code.
- **Queries:**
  - Rules and the fix sweep use `bounds()`.
  - Visibility is `get("opacity") > 0`.
  - The fix shift is 0 on an axis whose `follows_at` is false.
- **`at` output:** `id x=… y=… w=… h=…`, followed by every other registered property, in registry order, whose value differs from its default. So a resting `title` prints `title x=960 y=260 w=580.8 h=115.2`, and mid-entrance it also prints `opacity=0.5`. Update the README "Try it" output and the CLI tests to match.

- [ ] **Step 1: Tests first.**
  - Bounds under `scale` 2, `scale` −1 and `rotate` 90 and 45.
  - Opacity clamped under an overshooting curve, while `y` overshoots.
  - `pop` and `spin-in` evaluate as their data says.
  - Checks catch a rotated box leaving the canvas.
  - There is no fix shift on an `anim`-driven axis.
  - The `at` output format.
  - The existing convergence and settle tests still pass.
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run all three checks, plus the README commands. Commit:** `refactor(eval): evaluate tracks generically and check transformed bounds`.

---

### Task 6: Edits on nested keys, with JSON values

**Files:**
- Modify:
  - `crates/motion-edit/src/parse.rs`
  - `crates/motion-edit/src/apply.rs`

**Interfaces:**
- **Keys can be paths:** `anim.opacity`, `presets.my-pop`.
  - The first segment is checked against `LAYER_KEYS` or `SCENE_KEYS`; deeper segments are checked by `scene_from_value`.
  - Setting a path creates the objects it needs.
  - Removing the last key of an object removes the object, so `anim` never stays behind as `{}`.
- **A value can be JSON:** a value starting with `[` or `{` is read as JSON up to its balanced end. Strings inside it may contain spaces. Example: `card anim.opacity=[["0f",0],["12f",1,"ease-out"]]`.
- **Errors:**
  - Unbalanced JSON gives `unclosed \`[\` in …`, with the expected form.
  - Invalid JSON shows serde's message and the expected form.

- [ ] **Step 1: Tests first.**
  - Set and remove a nested key.
  - The empty-object cleanup.
  - JSON array and object values, including strings with spaces.
  - The error cases.
  - A full round trip through `apply_edit` and `scene_from_value`, with an `anim` keyframe array.
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run all three checks. Commit:** `feat(edit): edit nested keys and take JSON values`.

---

### Task 7: Documents

**Files:**
- Modify:
  - `docs/design/decisions.md`: add D27.
  - `docs/design/scene-format.md`
  - `docs/design/edits-and-checks.md`
  - `ARCHITECTURE.md`
  - `CHANGELOG.md`
  - `docs/glossary.md`: add track, key and curve, if missing.

**Content:**
- **D27. One generic property model.**
  - **Decision:** every animatable value is a registered property with a keyframe track; curves, presets and shorthands are data that expand into tracks.
  - **Why:**
    - masters can use any curve on any property;
    - adding a preset or a property is a data change;
    - evaluation, checks and the future renderer see one model.
- **`scene-format.md`:**
  - the property table;
  - curves (the table plus `cubic` and `hold`);
  - presets as data, including scene `presets`;
  - `anim` in both forms;
  - the conflict rule.
- **`edits-and-checks.md`:** nested paths and JSON values.
- **`ARCHITECTURE.md`:** where the registry, presets and tracks live.
- **`CHANGELOG.md`:** an `[Unreleased]` entry.

- [ ] **Step 1: Write the documents.** They must be short, one topic per section, and match the code.
- [ ] **Step 2: Commit:** `docs: describe the generic property model (D27)`.
