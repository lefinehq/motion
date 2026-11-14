# Reading `.aep` projects: the foundation

**Goal:** motion reads any `.aep` project into a typed, lossless model, and tells exactly what it could not decode yet ([D30](../design/decisions.md#d30-read-aep-projects-directly)). Mapping the model into a motion scene, and rendering it, come in the next plans.

**Done means:**
- `motion inspect <file.aep>` prints the project: items, compositions, layers, and the property tree with static values, keyframes and expressions. `--json` prints the model itself.
- Every sample project we have reads without an error or a panic, and the report lists every opaque value by kind and count.
- What the reader reports matches the Lottie exports of the fixture projects (see Tests).
- CI is green, and coverage stays at 85% or more.

## Format reference

The RIFX layout is documented in the open, MIT licensed, in the `aep_parser` project's [`docs/file_format.md`](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md). That document is the spec for this work; link the section a decoder follows in its doc comment. Do not port code from any parser; write the decoders from the spec and the sample files.

## Design

One new crate, `motion-import`, with one module per layer of the format. It reads bytes; it never touches files, pixels or the scene format.

| Module | Job |
|---|---|
| `rifx` | The chunk tree: `RIFX` header, `Egg!` form, chunks and `LIST`s, with padding to even sizes. Generic: it knows no tag but `LIST`. |
| `record` | The fixed binary records (`head`, `cdta`, `ldta`, `tdb4`, `cdat`, `lhd3`, `ldat` items, `shph`, ...), as `binrw` structs, each a few declarative lines. |
| `model` | The typed project: items, compositions, layers, and the property tree. Serde `Serialize`, so `--json` and snapshot tests come for free. |
| `decode` | Walks the chunk tree into the model. The only place that knows which chunk goes where. |
| `report` | Counts what the model holds: layer kinds, effect match names, opaque values by reason. |

### The model

The property tree is generic, as in [D27](../design/decisions.md#d27-one-generic-property-model). A match name is data, and there is no type per property and no branch per match name. The shape, roughly:

- `Project`: bits per channel, file version, and a flat list of `Item`s with folder parents.
- `Item`: id, name, parent folder, and a kind:
  - `Folder`;
  - `Composition`: size, pixel aspect, frame rate, duration, background colour, work area and layers;
  - `Footage`: solid with its colour, file with its path, or placeholder, with size and frame rate.
- `Layer`:
  - id, name and kind (AV, text, shape, camera, light, null, adjustment, 3D model);
  - its source item, parent layer, in and out points, start time and stretch;
  - switches: enabled, 3D, solo, shy, guide, motion blur;
  - blend mode, track matte, and a root `Group`.
- `Group`: match name, display name, and children, each a `Group` or a `Property`.
- `Property`:
  - match name and display name;
  - a value, `Static(Value)` or `Animated(Vec<Keyframe>)`;
  - an expression with its enabled flag, and min and max where stored.
- `Value`:
  - a number;
  - a 2D or 3D vector;
  - a colour;
  - a path (vertices with in and out tangents, closed flag);
  - `Opaque { reason }` for anything not decoded yet (text documents, gradients, markers, orientation).
- `Keyframe`:
  - time and value;
  - in and out interpolation (linear, bezier, hold);
  - temporal ease per dimension (speed, influence);
  - spatial tangents, and the roving and continuous flags.

Times are seconds (`f64`), converted once from the rational pairs and the layer's time base, as the spec's "Times and rates" says. Frame rates are `f64`. Item and layer ids are kept as the file's ids, so parent links survive.

### Errors

Every failure is a `motion_format::Diagnostic` ([D28](../design/decisions.md#d28-standard-parsing-tools-and-one-diagnostic-type)). Its path is the chunk path, such as `LIST:Fold/LIST:Item[3]/LIST:Layr[0]/ldta`. Map `binrw` errors to a diagnostic in one place. A chunk the reader does not know is not an error: it is skipped, and the report counts it. The reader refuses a file only when it cannot follow the structure: no `RIFX`/`Egg!`, a size past the end of the file, or a required record too short to read.

### Entry point and CLI

- `motion_engine` gains `import(path) -> Result<motion_import::Project>`. It runs `read` and `decode` through `stage`, so logs and `--timings` work as for scenes.
- `motion inspect <file.aep>` prints a readable tree. Each animated property shows its keyframe count and time range; each opaque value shows its reason; the report comes at the end. `--json` prints the model and the report.

## Tests

- **Unit tests** in `crates/motion-import/tests/unit/`, one file per module. The chunk reader and the record decoders are tested on hand-built byte strings.
- **Property tests:** the chunk reader never panics on arbitrary bytes, nor on any prefix of a real fixture.
- **Fixtures:** commit a handful of small sample projects from `airbnb/lottie-android` (Apache-2.0) and `boltframe/aftereffects-aep-parser` (MIT) under `crates/motion-import/tests/fixtures/`, about 2 MB in total. Add a `NOTICE.md` that names each file's source and licence.
- **Cross-check with Lottie:** for every fixture that has a Lottie export, commit the JSON and assert that both agree on:
  - the composition's size, frame rate, in point and out point;
  - each layer's name, in and out points and parent;
  - the keyframe times and values of the transform properties.

  This checks the decoders against an independent reading of the same project.
- **Snapshots:** an `insta` snapshot of `inspect` for each fixture.
- **Local sweep:** an `#[ignore]` test reads every `.aep` under `$MOTION_AEP_SAMPLES` and fails on any error or panic. Run it with `cargo test -- --ignored`. Never commit projects whose licence forbids redistribution.

## Out of scope

- Mapping into a motion scene. The report's "unsupported" column comes with that plan.
- Decoding text documents, gradients, markers and effect-specific records. They stay `Opaque` with a reason, so the report shows how often each one appears.
- Writing `.aep` files.

## Steps

1. Crate skeleton, `rifx` chunk tree, its property tests.
2. `head` and project version; folders and items (`LIST:Fold`, `LIST:Item`, `idta`, names), with compositions (`cdta`) and footage (`LIST:Pin`, `sspc`, `opti`).
3. Layers (`LIST:Layr`, `ldta`, names).
4. The property tree (`tdgp`, `tdmn`, `tdbs`, `tdsb`, `tdsn`, `tdb4`, `cdat`, expressions, `tdum`).
5. Keyframes (`LIST:list`, `lhd3`, `ldat`), for every stream kind in the spec's table.
6. Paths (`LIST:om-s`, `LIST:omks`, `shph`, `LIST:shap`).
7. `report`, `motion_engine::import` and `motion inspect`.
8. Fixtures, the Lottie cross-check, snapshots and the local sweep.
9. Documents:
   - `ARCHITECTURE.md`: crate table and reading order;
   - `CHANGELOG.md`;
   - a short `docs/design/aep-import.md`: what is decoded, what is opaque, and how to add a decoder.
