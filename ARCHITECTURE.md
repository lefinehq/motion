# Architecture

motion turns a scene into video in five steps. Steps 1 to 3 never touch pixels. The full reasoning is in [docs/design/rendering.md](docs/design/rendering.md).

```
scene.json
   │
   ▼
 1. Read      parse the JSON, expand presets and short strings, validate
   │
   ▼
 2. Compile   expand repeats, nested comps and layout; shape text  ──► checks and edits work from here
   │
   ▼
 3. Evaluate  for frame t: every layer's exact position, size, colour and opacity
   │
   ▼
 4. Draw      rasterise each layer, then composite: effects, masks, mattes, blend modes, depth
   │
   ▼
 5. Encode    turn frames into MP4, WebM, MOV or PNG files, and mix in audio
```

## Crates

Each crate has one job and a list of things it must never do. Keeping those rules is what keeps the pipeline testable.

| Crate | What it does | What it must never do |
|---|---|---|
| `motion-format` | Reads the scene format: shorthand units, parsing and validation (step 1). | Touch pixels. |

Crates are added task by task. This table lists only crates that exist.

### Planned crates (not yet created)

| Crate | What it will do |
|---|---|
| `motion-raster` | Rasterise shapes and text into pixels (step 4). |
| `motion-composite` | Combine layers: effects, masks, mattes, blend modes (step 4). |
| `motion-encode` | Write video and audio files (step 5). |
| `motion-import` | Bring in assets and other formats. |
| `motion-variants` | Turn one designed project into many variants. |

## Where to start reading

1. `crates/motion-format/src/lib.rs`: the scene format and its units.
2. `crates/motion-eval/src/evaluate.rs`: how a frame's values are worked out (not yet created).
