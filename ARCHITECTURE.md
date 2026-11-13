# Architecture

motion turns a scene into video in five steps. Steps 1 to 3 never touch pixels. Checks, edits and the command-line tool are a side branch that works on the output of steps 2 and 3. The full reasoning is in [docs/design/rendering.md](docs/design/rendering.md).

```
scene.json
   │
   ▼
 1. Read      parse the JSON, expand presets and short strings, validate
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
| `motion-format` | Reads the scene format: shorthand units, parsing, validation and writing (step 1). | Touch pixels. |
| `motion-scene` | Compiles a scene into a scene graph with resolved positions, sizes and entrances (step 2). | Touch pixels or read files. |
| `motion-eval` | Works out every node's position, size and opacity at a frame (step 3). | Touch pixels or read files. |
| `motion-query` | Side branch: `check` and `at` answer questions about a scene with numbers. | Write files or touch pixels. |
| `motion-edit` | Side branch: parses one-line edits and applies them to the raw scene. | Write files or touch pixels. |
| `motion-cli` | The `motion` command: wires the crates together. | Hold engine logic. It is the only place that reads or writes files and prints. |

Crates are added task by task. This table lists only crates that exist.

### Planned crates (not yet created)

| Crate | What it will do |
|---|---|
| `motion-raster` | Rasterise shapes and text into pixels (step 4). |
| `motion-composite` | Combine layers: effects, masks, mattes, blend modes (step 4). |
| `motion-encode` | Write video and audio files (step 5). |
| `motion-import` | Bring in assets and other formats. |
| `motion-variants` | Turn one designed project into many variants. |

## One property model

Every animatable value is a registered property with a keyframe track (see [D27](docs/design/decisions.md#d27-one-generic-property-model)). The pieces live here:

| What | Where |
|---|---|
| The property registry: name, default, range, meaning | `crates/motion-format/src/property.rs` |
| Curves: the named table, `cubic(...)` and `hold` | `crates/motion-format/src/curve.rs` |
| Presets, as data (a scene can add its own) | `crates/motion-format/src/presets.json`, read by `preset.rs` |
| `anim` tracks and keyframes, as written | `crates/motion-format/src/anim.rs` |
| Tracks: one property's keys, compiled and sampled | `crates/motion-scene/src/track.rs` |

The rule: **names are data; add a row, not a branch.** A new property is a row in the registry, a new preset is an entry in `presets.json`, a new curve name is a row in the curve table. Code reads these tables and never lists the names itself, and errors about an unknown name list the allowed ones from the same table.

## Where to start reading

1. `crates/motion-format/src/lib.rs`: the scene format and its units.
2. `crates/motion-eval/src/evaluate.rs`: how a frame's values are worked out.
