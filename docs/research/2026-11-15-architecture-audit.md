Status: Draft · 2026-11-15

# Architecture audit: what the variant roadmap needs changed first

We read every crate against [CONTRIBUTING](../../CONTRIBUTING.md), the [decisions](../design/decisions.md) and the [variant roadmap](../design/variants.md). The goal was to flag code that solved a case for the day instead of building the shape we will need, before more features land on it. Nothing was changed. This note lists the findings and the order to fix them in.

**The one thing to remember:** motion has no address for a layer. Edits, `at`, `check`, render errors, the import report and the fidelity tools each name layers their own way, and every one of them reaches top-level layers only. That is a problem because, in our local corpus of 198 templates, 121 keep all their text inside nested comps. Most other findings follow from that, or from the scene being built and edited as untyped JSON.

## Why the audit was needed

A scan of the 198-template local corpus with `motion inspect --json` (*measured*, 2026-11-15):

| What a variant has to reach | Templates | Count |
|---|---|---|
| Text layers inside nested comps | 128 (121 of them have all their text there) | 1,343 layers |
| Text layers at the top level | 43 | 96 layers |
| Colour controls on a control layer | 108 | 1,322 controls |
| Expressions that read another layer | 108 | 3,474 expressions |
| Slider, checkbox and point controls | 69 / 28 / 15 | 534 / 57 / 63 |
| Boxes sized from their text | 15 | 130 expressions |
| Slots the designer exposed on a comp instance | 1 | 8 |

These templates are built for variants: the designer gathers colours, sizes and switches on a control layer, and expressions wire the content to it. The roadmap needs three things:
- a way to address that structure;
- a way to keep the links live;
- a way to edit the templates in batches.

## The workstreams, in fix order

Each workstream lists the findings it closes, by crate and line on `main` at `d470e05`, after PR #34. **Sev** is how much the finding blocks the roadmap: **high** means it blocks it or will force a rewrite; **medium** means it must be fixed but does not block; **low** means it is cleanup.

### 1. One layer address

There is no address type. Paths are strings built with `format!` in each crate, and the spellings already disagree:
- `layers[1].rect` in diagnostics;
- `comp/layer` in the import report and the CLI;
- `(comp: Option<String>, layer)` in `StoredText`;
- a bare id in edits, `hiding` and `ignore`.

**Target shape:** a typed `LayerPath` (the comp chain, then a layer id, later an item or slot) in motion-format, with a `winnow` grammar, `Display` and `FromStr`.
- It is built on an `Id` newtype that holds the id rule.
- `Diagnostic.path` becomes a list of segments (key, index, id), printed only by `Display`.
- One resolver maps a path onto the document, the `Scene` and the `SceneGraph`.

This needs a decision (D43): the separator, and how a comp instance is told apart from the comp it shows.

| Where | Sev | What is wrong |
|---|---|---|
| `motion-edit/src/apply.rs:47-70` | high | `Target` is `Scene \| Layer(usize)` and `find` searches only `scene().layers`. |
| `motion-edit/src/parse.rs:33-38, 141-151` | high | The target is a public `String`, read as "anything up to a space or `=`". It doesn't reuse the id rule. |
| `motion-edit/src/path.rs:15-34` | high | Key paths cannot step into arrays (`fx[glow].radius`, `runs[0]`). The only way to change a nested layer is to rewrite a whole `comps.x.layers` array. |
| `motion-format/src/validate.rs:126-152`, `scene.rs:141` | high | Layer ids are unique only within their own list, and `Content::Comp(String)` is a bare id. |
| `motion-format/src/diagnostic.rs:69, 129-142` | medium | The path is a `String` such as `comps.card.layers[1]`. An agent cannot turn it into an edit target. |
| `motion-format/src/validate.rs:422-427`, `shorthand/behaviour.rs:17` | medium | The id rule is a hand-written `chars()` loop, and the behaviour shorthand imports it from `validate`. |
| `motion-query/src/at.rs:16-35` | high | `at` searches top-level nodes, takes `&[String]` and returns one preformatted `String`. |
| `motion-engine/src/session.rs:351-363`, `diff.rs:47-49`, `fidelity.rs:52-55`, `geometry.rs:191-244` | high | `hiding`, `ignore` and the text geometry address layers in three ways, and none of them can reach a nested layer. |
| `motion-engine/src/render/mod.rs:128, 149` | low | Render errors name a bare `node.id` and drop the comp. |
| `motion-import/src/map.rs:292-313`, `map/ids.rs:18-45` | high | A scene id is an ASCII slug whose suffix depends on file order. Source item and layer ids are dropped. Non-ASCII names become `layer-N`. |
| `motion-import/src/map.rs:299-303` | medium | `Named.comp` is documented as a scene id but holds the project comp's name. |
| `motion-import/src/map/items.rs:23-41`, `map/report.rs:40-46, 79-81` | medium | Findings and expression reports are keyed by a prose `name/name/match-name` path. |
| `motion-cli/src/commands/import.rs:115-118` | medium | The CLI prints a third spelling, `comp/layer`. |
| `motion-edit/src/apply.rs:14, 59, 148` | medium | `scene` is a reserved target but a valid id, and a layer named "Scene" imports as `scene` and can never be edited. |

### 2. One walk over the comp tree

The scene root and a nested comp are two types (`Scene` and `Comp`), validated by two near-copies. Only the renderer goes into nested comps. Every other consumer sees the top level only.

**Target shape:**
- the scene is a root `Comp` plus a few scene-only fields;
- motion-scene gains one iterator over every node, which yields each node's address, scene-space transform and visibility;
- render, `check`, `at` and geometry all use it.

| Where | Sev | What is wrong |
|---|---|---|
| `motion-format/src/scene.rs:19-56`, `validate.rs:80-120` vs `177-202` | medium | `Scene` and `Comp` repeat size, frame rate, duration, background and layers. |
| `motion-scene/src/evaluate.rs:79-87`, `state.rs:58-60` | high | A nested comp's state is a boxed `FrameState` in local space, and only `motion-engine/src/render/mod.rs:130` composes it. |
| `motion-query/src/check.rs:24-44, 226-229`, `rules/bounds.rs:33-41` | high | `check` never looks inside a nested comp, so most template text is never checked for offscreen, safe-area or overflow. |
| `motion-query/src/check.rs:25, 43` | medium | `check` zips `Scene` and `SceneGraph` by index, held in step only by a `debug_assert`. |

### 3. Typed writing

The import builds the scene as untyped JSON with about 38 literal keys and hand-printed shorthands. The format's `Display` rounds to 2 decimals, so it cannot be the writer either. The import therefore prints curves, paths, fonts and styles by hand, and its copies have already drifted from the format's.

**Target shape:**
- one lossless writer per typed value in motion-format, with `fmt_num` used only for output people read;
- a typed `Scene` → JSON writer;
- the import builds typed values;
- `Edit` gets a `Display`, so fix hints are structured edits.

| Where | Sev | What is wrong |
|---|---|---|
| `motion-format/src/value.rs:255-288`, `curve.rs:202-235`, `path.rs:224-249` | high | `Display` is lossy, and the round-trip proptests round their inputs first (`tests/unit/value.rs:151`). |
| `motion-format/src/write.rs:18-52` | medium | The canonical writer slices serialized strings and pastes them together. It knows only `layers` and `comps`. |
| `motion-import/src/map/layer.rs:77-291`, `map.rs:219-273`, `map/text.rs:70-150` | medium | Literal keys, and shorthands formatted with `format!`. `motion_format::key` is used in one file only. |
| `motion-import/src/map/keys.rs:206-231, 302` | medium | `speed(…)`, `hold` and `linear` are printed by hand, apart from `Curve`'s `Display`. Since PR #34, `speed(…)` is printed in two places. |
| `motion-import/src/eval.rs` (new in PR #34) vs `motion-scene/src/curve.rs:73-95`, `track.rs:139-210` | high | The bake reads a property's value through a second keyframe evaluator, "as the map writes them for the scene". It re-implements the scene's ease solver, linear and hold sides, and its arc-length walk along spatial paths. The two must agree by hand, and when they drift a baked expression reads values the scene never draws. | The bake samples the mapped tracks through motion-scene, so it sees what the scene draws by construction. Map first, then bake on the mapped values. |
| `motion-import/src/map/text.rs:215-262` | medium | The style names and defaults are copied, and have drifted: **a stroked run of width 0 imports as width 1**. |
| `motion-import/src/map.rs:53-57`, `map/fonts.rs:44-85` | medium | `Import` holds both the raw JSON and the `Document`. `substitute_fonts` patches the JSON afterwards with `split_once` and `format!`. |
| `motion-engine/src/fonts.rs:78-104` | medium | `add_fonts_entry` edits raw JSON and copies the writer's key order. |
| `motion-query/src/check.rs:263`, `motion-edit/src/apply.rs:142`, `problem.rs:92` | medium | Edit lines are built by hand in three crates, and `Violation.fix` is an `Option<String>`. |

### 4. An engine session for import and variants

Import is put together twice, in the CLI and in `fidelity`. Each time it writes the scene to disk and opens a `Session` only so font paths resolve. The only way to change a scene is `edit(&str)`: one line per call, and every call drops every cache, recompiles and runs a full check.

**Target shape:**
- `Session::from_import` holds the scene in memory with its base folder, and runs footage, map, fonts, substitution and geometry as stages;
- baking and slot extraction become later stages of the same session;
- typed edits are applied in batches, with one check per batch, and drop only the caches they touch;
- `motion edit` takes several lines and an `--out`, so the master stays untouched.

| Where | Sev | What is wrong |
|---|---|---|
| `motion-engine/src/fidelity.rs:208-215`, `motion-cli/src/commands/import.rs:53-70` | high | The import pipeline is written twice, and goes through the disk. |
| `motion-engine/src/session.rs:338-344` | high | One string edit per call; a full cache drop, compile and check every time. |
| `motion-cli/src/commands/edit.rs:12-16` | medium | One line per run; it overwrites the master and checks in full. |
| `motion-cli/src/commands/import.rs:53-70`, `fonts.rs:106-156` | medium | The CLI holds pipeline logic and reads scene JSON itself, against ARCHITECTURE. |

### 5. Live values: controls, slots and links

This is the core of the product, and today the import removes it before baking even runs:
- control effects are mapped as "ignored, draws nothing";
- so is the comp-instance override group, where the designer exposes slots;
- effect and group names are dropped;
- the report lists all of them as harmless.

The scene has no slot or reference value, comp instances cannot override anything, and compile has no order between layers. A value derived from another layer therefore has nowhere to live, whether it is a box sized from its text or a colour read from a control.

**Target shape:**
- scene-level parameters (slots), typed by `ValueType`, and a value form that reads one;
- controls and comp-instance overrides become slots;
- a comp layer carries overrides keyed by inner path;
- compile runs in dependency order, with a "derived from node X, property P" track source.

This needs a decision that refines D38: an expression that only reads controls stays a live link, and anything else is still baked. See the note on PR #34 below.

| Where | Sev | What is wrong |
|---|---|---|
| `motion-import/src/map/rows.rs:662-689` | high | Slider, angle, checkbox, colour, point and layer controls are dropped. The control layer becomes a bare `pivot`. |
| `motion-import/src/map/rows.rs:627-631` | high | Comp-instance overrides and the data group are ignored as "drawing nothing", but overrides do change frames. |
| `motion-import/src/map/items.rs:306-330` | high | `item()` never writes `Group.name`. "Headline Color" and "Logo" are lost. |
| `motion-format/src/scene.rs:89`, `item.rs:416-423` | high | A value is a literal or a track. There is no slot form and no control effect kind. |
| `motion-scene/src/compile.rs:86-131, 291-322` | high | Compile is single-pass, and each box comes only from its own content. |
| `motion-import/src/map/report.rs:76-90` | medium | Since PR #34 each expression has a report, but it keeps only the source's first line, and its place is a prose path. A live link cannot be built from it later. |
| `motion-import/src/map/rows.rs:711`, `decode/property.rs:86-88`, `model.rs:70-88` | medium | Markers stay opaque, and comps have none. They are the designer's own beats, which cut-downs need. |

### 6. The registry covers every value (D27)

D27 says every animatable value is a registered property. Today only the five transform rows are. Fill and text style are struct fields with a separate setter table, and item settings are strings matched by name. Several enums are defined two or three times across format, import, text and raster.

**Target shape:**
- content values are registry rows;
- settings are typed enums with strum names;
- there is one colour type, the vocabulary type under D37;
- a format-to-engine mapping is an exhaustive match, so a missing row fails to compile instead of panicking.

| Where | Sev | What is wrong |
|---|---|---|
| `motion-format/src/property.rs:183-211`, `scene.rs:84`, `text.rs:55-80` | high | Fill and text style cannot be animated, targeted by `anim` or addressed generically. |
| `motion-format/src/item.rs:48-57, 159-195`, `motion-scene/src/animate.rs:166-221`, `motion-engine/src/render/shape.rs:28, 57, 76` | medium | Settings are `&[&str]` read by name, with `_` and `unreachable!` fallbacks and an `expect` on a mismatch. |
| `motion-format/src/validate.rs:429-520` | medium | The content kind is chosen by a match on key strings, with a parallel `written` array and five `expect`s. |
| `motion-format/src/units.rs:187-198`, `value.rs:91`, `motion-raster/src/style.rs:9-46` | medium | There are four colour types. `units::Color` is `u8` and quantises backgrounds. |
| `motion-format/src/text.rs:185-208, 361-433`, `shorthand/matte.rs:13-45` | medium | The name tables are written by hand, and `MatteMode::name` falls back to `"alpha"` without a word. |
| `motion-text/src/typeset.rs:90-95`, `caps.rs:91-94`, `motion-raster/src/style.rs:119-136`, `blend.rs:147-172` | medium | Each row is found by a linear search plus `expect("every X has a row")`. |
| `motion-engine/src/render/mod.rs:104-109`, `motion-text/src/style.rs:214, 277, 296` | medium | `Caps`, `Justification`, `TextFrame` and `MatteMode` are defined more than once. |
| `motion-format/src/shorthand/behaviour.rs:235-258` | medium | Curve tokens are recognised by their shape, so `speed(…)` is rejected in `in`. |
| `motion-import/src/map/items.rs:376-400`, `map/layer.rs:45-48`, `map/rows.rs:609-611` | low | Match names are spelled across several modules. Layer styles are sorted by a prefix test. |

### 7. Text that survives a swap

Styled runs are fixed character ranges, and a range past the end is an error. Swapping or localising the text of any styled layer therefore fails validation, or puts the styles on the wrong characters. Each run also has exactly one face, which rules out picking fonts per script.

**Target shape:**
- runs anchored to the text (a paragraph, a word, the rest of the text);
- one text-slot operation that owns the text together with its runs;
- a declared fallback list on a style, reported when it is used (D33).

| Where | Sev | What is wrong |
|---|---|---|
| `motion-format/src/validate/text.rs:55, 182-201`, `text.rs:41-51` | high | Character-index runs reject a shorter replacement. |
| `motion-edit/src/apply.rs:99-112` | high | `text=` replaces only the string, and leaves the runs as they were. |
| `motion-text/src/style.rs:156` | medium | `CharacterStyle.font` is one face, so localise has no fallback stack. |
| `motion-engine/src/fonts.rs:56-61`, `motion-text/src/set.rs:152-154`, `fallback.rs:139-145` | low | "Has this font" is decided three ways. |

### 8. Diagnostics and reports as data

Context such as "frame N:" or "sample `id`:" is added by rewriting the message. That is why tests check the wording. Findings are free text, and the ledger counts them by that text, font names included, so rewording a finding changes the regression numbers.

**Target shape:**
- context is a field (or `under`) on `Diagnostic`, never part of the message;
- a finding is a typed kind plus a detail, and the ledger counts by kind;
- the inner diagnostic is kept, not flattened.

| Where | Sev | What is wrong |
|---|---|---|
| `motion-engine/src/session.rs:295`, `pictures.rs:49-52`, `fidelity.rs:190-193` | medium | Context is put into the message with `format!`. |
| `motion-engine/src/fidelity.rs:227-230` | medium | The ledger groups findings by their text. |
| `motion-import/src/map.rs:104-111` | medium | `read_back` flattens the format's diagnostic and loses its kind, path and expected form. |
| `motion-engine/src/diff.rs:96-143, 211-221`, `fidelity.rs:108-130, 256-271, 321-358` | medium | Three windows × two metrics are written out field by field. Metric names and direction are string literals. |
| `motion-cli/src/commands/render.rs:35-56` | medium | `Frames` is parsed with `split_once('-')` into a `String` error. |
| `motion-edit/src/path.rs:23-26` | medium | "`in` is not an object" has no path and no expected form. |
| `motion-engine/src/fidelity.rs:71-75, 150-151` | low | Manifest and ledger errors have no key path. |
| `motion-import/src/decode/effect.rs:122-133`, `decode/text.rs:31-35, 495` | low | Messages are templated with `replace`, and paths are joined by hand. Chunk paths use `/` while `under` joins with `.`. |
| `motion-cli/src/commands/import.rs:69, 77-82`, `diff.rs:43-48`, `inspect.rs:26-31`, `fonts.rs:148-153` | low | Four hand-written JSON error wrappers, and a diagnostic turned into a string inside a report. |

### 9. Guards that guard

The checks for D37, D38 and D39 grep source and manifest lines and look only at direct dependencies. Since PR #34, motion-import depends on motion-expr, so motion-engine, the crate that renders, and the CLI link the script engine (`cargo tree -i boa_engine`). No render calls it, but the check passes only because motion-engine is left out of the crates it checks.

**Target shape:**
- baking behind a `bake` feature of motion-import that only the CLI turns on, the same pattern D39 uses for `fetch`, so a crate that renders but does not import never links the script engine;
- cargo-deny `[bans]` with `wrappers` for the script engine and the font fetcher;
- a structural public-API check instead of grep;
- workspace lints against `unwrap`, `expect`, `panic` and truncating casts;
- a CI job that builds the WebAssembly-ready crates for `wasm32`, plus per-feature builds.

| Where | Sev | What is wrong |
|---|---|---|
| `motion-engine/tests/boundaries.rs:87-101, 136-153` | high | Cargo.toml is parsed line by line, direct dependencies only, and the engine is left out of `DRAWS`. Since PR #34 the engine links the script engine and the test still passes. |
| `motion-engine/tests/boundaries.rs:59-63`, `motion-text/src/layout.rs:13`, `font.rs:8`, `motion-raster/src/list.rs:6-7` | medium | Public fields are typed through `parley` and `vello_cpu`, and the check misses them. |
| `Cargo.toml:15-19` | medium | Lints are `clippy::all` only. 27 `as` casts go unchecked. |
| `.github/workflows/ci.yml:14-31` | medium | No `wasm32` build. Feature unification hides motion-fonts without `fetch`. |
| `motion-raster/Cargo.toml:12`, `motion-text/Cargo.toml:13` | medium | The raster and text crates pull in the whole scene format only for `Diagnostic`. |
| `Cargo.toml:90-91` vs `motion-raster/Cargo.toml:14` | low | `vello_cpu` is declared twice. |

### 10. Tests where the corpus is

No committed test covers nested comps, the most common shape in the corpus. The import fixtures produce no `comps`, and edit, `at` and `check` never see one.

| Where | Sev | What is wrong |
|---|---|---|
| `motion-import/tests/import.rs`, `tests/unit/map.rs` | high | No nested project, no control layer and no named effect. |
| `motion-edit/tests/unit/apply.rs`, `motion-query/tests/unit/*` | medium | No scene with `comps`. |
| `motion-cli/tests/cli.rs` | medium | Nothing covers `import`, `diff`, `fidelity` or `fonts`. |
| `motion-engine/tests/unit/*`, `motion-raster/tests/unit/render.rs:52` | medium | Tests check `message.contains`, and these crates have no snapshot of their error wording. |
| `motion-text/tests/samples.rs:41-151` | medium | It re-implements the import-to-text mapping, so it tests its own copy. |
| `motion-format/src/shorthand/matte.rs` | low | No unit tests and no never-panic proptest. |
| `motion-format/tests/unit/*` | low | 39 tests loop over cases instead of one `#[rstest]` per case. |

### 11. Docs that predate D30 and D38

| Where | Sev | What is wrong |
|---|---|---|
| `docs/design/variants.md` | high | It describes the D24 exporter (its anchor is broken), says expressions become `jitter`/`loop`/`link` modifiers, lists reading `.aep` directly as "later", and says nothing of nested comps or control layers. |
| `overview.md:10`, `scope.md:44, 78`, `features.md:56`, `roadmap.md:13, 40, 97-98`, `quality.md:83` | medium | They still describe the exporter and the deferred reader. |
| `scene-format.md:339` | medium | It says system fonts work. D14, D33 and `FontSet` say they are never read. |
| `scene-format.md:320-330, 344` | low | It documents modifiers and a JSON Schema that no crate has. |
| `motion-import/src/lib.rs:6`, `Cargo.toml:3`, `ARCHITECTURE.md:36`, `docs/design/aep-import.md:5` | low | The crate is described as a reader that never touches scenes, but `map` writes them. |
| `motion-engine/src/render/text.rs:27, 34`, `diff.rs:17, 53` | low | Other products are named outside a benchmark (D18). |
| `CONTRIBUTING.md`, "Adding a field" | low | It says `Option<Shorthand<T>>`, but the fields are `Present<…>`. |

## Bugs found on the way

These are wrong today, whatever the roadmap. Each gets a test that reproduces it.

| Where | What happens |
|---|---|
| `motion-import/src/map/text.rs:215-262` | A stroked run of width 0 imports as width 1, because the import's default stroke width differs from the format's. |
| `motion-import/src/record.rs:249-265`, fixture `Layer-01` | The collapse switch is not decoded, and the layer "Transformations Collapsed" reads as `solo`. Either the solo bit is mislabelled or the fixture really is soloed. |
| `motion-import/src/map/layer.rs:167-176` | A layer whose source is missing is dropped silently. The branch meant to report it can never run. |
| `motion-import/src/map/layer.rs:236-238` | A parent outside the layer's comp is dropped with no finding. |
| `motion-import/src/map.rs:114-165` | `--comp` takes the first comp with a matching name, even when two have it. |
| `motion-edit/src/apply.rs:14, 59` | A layer with the id `scene` cannot be edited. |
| `motion-engine/src/diff.rs:305, 325, 356` | Two `pixels()` helpers round differently, and `diff` renders outside the `render` stage. |

## After PR #34: D38 and the variant surface

PR #34 bakes every expression into keys. That is right for expressions driven by `time`, `wiggle` or `loopOut`. But in 108 templates, expressions read a control layer. Baking those keeps the pixels and loses the variant surface: the colour a designer set on one control is copied into a few hundred keys across many layers.

D38 already names this case under **Not now**: live expressions "would be a separate decision, made only if a real template needs a value to follow an edit". The corpus shows that 108 templates need exactly that, so the separate decision is D44.

What PR #34 brings that D44 can build on:
- **The model decodes controls.** Slider, angle, checkbox, pop-up, colour and point controls are decoded with their defaults, and the bake reads them (`bake/host.rs:447`). The map still drops them (workstream 5).
- **Each expression has a report:** its engine, its frames and its outcome. Its source and its place still need to become data (workstreams 1 and 5).

Where PR #34 diverges from this audit:
- **A second keyframe evaluator,** `motion-import/src/eval.rs` (workstream 3). *Resolved in PR #36:* the bake now eases with motion-scene's `Ease` and `PathSpan`.
- **The script engine is now in the render crate's dependencies,** and the guard misses it (workstream 9).
- **`speed(…)` is printed by hand in a second place** (workstream 3). *Resolved in PR #36:* the map and the bake share one curve per span, printed in one place.

## What comes next

1. **Fix the bugs above,** one small PR each, and put baking behind a `bake` feature (workstream 9).
2. **Decision D43: the layer address.** Then workstreams 1 and 2. This is the first item on the variant list: editing text inside nested comps.
3. **Workstreams 3 and 4.** Typed writing and the import session remove the JSON patching that the slot work would otherwise build on.
4. **Decision D44: live control links, refining D38.** Then workstream 5, and workstream 7 for text swaps.
5. **Workstreams 6, 8, 9, 10 and 11** go alongside, as cleanup PRs. Each one is scoped to a workstream, not to a file.
