# Architecture

motion turns a scene into video in five steps. Steps 1 to 3 never touch pixels. Checks, edits and the command-line tool are a side branch that works on the output of steps 2 and 3. Every tool (the command line today, an agent server and a Node binding later) reaches the pipeline through one entry point, the engine's `Session`. The full reasoning is in [docs/design/rendering.md](docs/design/rendering.md).

```
tools (CLI, later MCP and Node) ──▶ Session (motion-engine) ──▶ the steps below
scene.json
   │
   ▼
 1. Read      parse the JSON (serde), read short strings (winnow grammars), validate
   │
   ▼
 2. Compile   expand repeats, nested comps and layout; shape text
   │
   ▼
 3. Evaluate  for frame t: every layer's exact position, size, colour and opacity ──┐
   │                                                                                │
   ▼                                                                                ▼
 4. Draw      rasterise each layer, then composite:             side branch: checks, edits and the CLI
   │          effects, masks, mattes, blend modes, depth        work on the output of steps 2 and 3;
   ▼                                                            they are not a numbered rendering step
 5. Encode    turn frames into MP4, WebM, MOV or PNG files, and mix in audio
```

## Crates

Each crate has one job and a list of things it must never do. Keeping those rules is what keeps the pipeline testable.

| Crate | What it does | What it must never do |
|---|---|---|
| `motion-diagnostic` | The one error type, `Diagnostic`, with a `Kind` and an exit code. | Depend on any other motion crate, or format an error anywhere else. |
| `motion-grammar` | The shared `winnow` pieces every text grammar uses, and the one mapping from a grammar's failure to a `Diagnostic`. | Expose `winnow` types in other crates' public APIs. |
| `motion-format` | Reads the scene format: a serde document for the JSON shape, `winnow` grammars for the shorthands, then validation and writing (step 1). | Touch pixels. |
| `motion-scene` | Compiles a scene into a scene graph with resolved positions, sizes and entrances (step 2). | Touch pixels or read files. |
| `motion-eval` | Works out every node's position, size and opacity at a frame (step 3). | Touch pixels or read files. |
| `motion-query` | Side branch: `check` and `at` answer questions about a scene with numbers. | Write files or touch pixels. |
| `motion-edit` | Side branch: parses one-line edits and applies them to a parsed scene's raw JSON. | Write files or touch pixels. |
| `motion-engine` | `Session`: the only entry point for tools. It reads, parses, compiles and evaluates a scene and runs `check`, `at` and edits, each step inside a `tracing` span. | Print, read arguments, or let a tool bypass it to call a step directly. |
| `motion-telemetry` | Starts logging and step timings (`--log`, `MOTION_LOG`, `--timings`) from the spans the engine opens. | Be started by a library: only binaries and tests install a subscriber. |
| `motion-cli` | The `motion` command: reads the arguments, runs each command through a `Session`, and prints its output. | Hold engine logic. It is the only place that prints, and it reaches scenes only through `Session`. |

Crates are added task by task. This table lists only crates that exist.

### Planned crates (not yet created)

| Crate | What it will do |
|---|---|
| `motion-raster` | Rasterise shapes and text into pixels (step 4). |
| `motion-composite` | Combine layers: effects, masks, mattes, blend modes (step 4). |
| `motion-encode` | Write video and audio files (step 5). |
| `motion-import` | Bring in assets and other formats. |
| `motion-variants` | Turn one designed project into many variants. |

## Parsing, validation and errors

Libraries never print: they return a `Diagnostic` or log with `tracing`, and only `motion-cli` writes to stdout and stderr.

| What | Where |
|---|---|
| The error type, its kinds and exit codes | `crates/motion-diagnostic/src/lib.rs` |
| Shared grammar pieces and the failure-to-`Diagnostic` mapping | `crates/motion-grammar/src/lib.rs` |
| The scene's JSON shape, as serde types (`SceneDoc`, `LayerDoc`) | `crates/motion-format/src/doc.rs` |
| Shorthand grammars: units, rect, font, position, behaviour, tracks | `crates/motion-format/src/units.rs`, `shorthand/`, `anim.rs`, `curve.rs` |
| Validation: every rule beyond the shape | `crates/motion-format/src/validate.rs` |
| The edit line grammar and path edits | `crates/motion-edit/src/parse.rs`, `apply.rs` |
| The entry point and its step spans | `crates/motion-engine/src/session.rs`, `stage.rs` |

See [Contributing](CONTRIBUTING.md#how-errors-work) for how to add a shorthand or a field.

## One property model

Every animatable value is a registered property with a keyframe track (see [D27](docs/design/decisions.md#d27-one-generic-property-model)). The pieces live here:

| What | Where |
|---|---|
| The property registry: name, default, range, meaning | `crates/motion-format/src/property.rs` |
| Curves: the named table, `cubic(...)` and `hold` | `crates/motion-format/src/curve.rs` |
| Presets, as data (a scene can add its own) | `crates/motion-format/src/presets.json`, read by `preset.rs` |
| `anim` tracks and keyframes, as written | `crates/motion-format/src/anim.rs` |
| Tracks: one property's keys, compiled and sampled | `crates/motion-scene/src/track.rs` |

The rule: **names are data; add a row, not a branch.** A new property is a row in the registry, which covers parsing, `anim`, presets, edits, `at` output and clamping (a property with a geometric or visual effect also needs code in `NodeState::bounds()` and the renderer), a new preset is an entry in `presets.json`, a new curve name is a row in the curve table. Code reads these tables and never lists the names itself, and errors about an unknown name list the allowed ones from the same table.

## Where to start reading

1. `crates/motion-format/src/lib.rs`: the scene format and its units.
2. `crates/motion-eval/src/evaluate.rs`: how a frame's values are worked out.
