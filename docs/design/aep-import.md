Status: Draft · 2026-11-14

# `.aep` import

motion reads `.aep` projects itself, into a typed model that keeps everything it found: what it cannot decode yet stays in the model as an opaque value with its reason, and every chunk it passes over is counted in the import report ([D30](decisions.md#d30-read-aep-projects-directly)). Mapping the model into a scene comes later.

```
$ motion inspect Star.aep
project 88.1, 8 bits per channel
composition 1 "Star" 400x400, 60 fps, 4s, work area 0s-4s
  layer 13 "Shape Layer 1" shape, 0s-4s
    ADBE Root Vectors Group
      ADBE Vector Group "Polystar 1"
        ...
            ADBE Vector Star Points: 2 keyframes 0s-3.98s
    ADBE Transform Group
      ADBE Position = [203, 208, 0]
      ADBE Orientation = opaque orientation
report
  items: composition 1
  layers: shape 1
  effects: none
  opaque: orientation 1
  skipped: LIST:CLay 3, LIST:CPPl 1, ...
```

`motion inspect --json` prints the same model and report as JSON.

## What is decoded

| Part | Read from |
|---|---|
| File version and colour depth | `head`, `nhed` or `nnhd` |
| Folders, compositions and footage, with their folder | `LIST:Fold`, `LIST:Item`, `idta`, `cdta` |
| Footage: solids (colour), files (path), placeholders; size, rate, duration | `LIST:Pin `, `sspc`, `opti`, `LIST:Als2` |
| Layers: name, kind, source, parent, timing in composition seconds, switches, blend mode, track matte | `LIST:Layr`, `ldta` |
| The property tree: groups, effects, masks, names, enabled state | `LIST:tdgp`, `tdmn`, `tdsn`, `tdsb`, `LIST:sspc` |
| Numbers, vectors and colours, static or keyframed with interpolation, eases and spatial tangents | `LIST:tdbs`, `tdb4`, `cdat`, `LIST:list`, `lhd3`, `ldat` |
| Expressions, ranges | `Utf8` or `expr`, `tdum`, `tduM` |
| Mask and shape paths, static or keyframed | `LIST:om-s`, `LIST:omks`, `LIST:shap`, `shph` |

Text is read as UTF-8 inside `Utf8` from file version 92, and as raw, NUL-terminated text before it.

## What is opaque

A property whose value the reader keeps but cannot read yet is `opaque` with one of these reasons:

| Reason | What it is |
|---|---|
| `text_document` | A text layer's text and style (`LIST:btds`) |
| `gradient` | Gradient colours (`LIST:GCst`) |
| `markers` | Layer markers (`LIST:mrst`) |
| `orientation` | 3D orientation (`LIST:otst`) |
| `valueless` | A stream with no value, such as a layer reference |
| `dimensions` | A stream whose dimension count the reader does not handle |
| `other_body` | A property body not in the table |

Chunks outside the property tree that the reader does not need yet (views, render settings, guides, caches) are passed over and listed under `skipped`. Across the 37 local sample projects, 674 properties are keyframed, 11,371 are static, and 613 are opaque (538 of them orientations).

## How to add a decoder

1. Write the record as a `binrw` struct in `crates/motion-import/src/record.rs`, with each field at its offset (`#[br(seek_before = at(0x..))]`) and a doc comment linking the section of the format notes it comes from. Read it with `parse`, which is the only place a `binrw` error becomes a `Diagnostic`.
2. If the record is the body of a property, add a row to `BODIES` in `crates/motion-import/src/decode/property.rs`; to give an opaque body a reason, add the reason to `Reason` in `model.rs`. Codes that become enums go through the `coded!` macro, so a new code is a new variant, not a new branch.
3. Read it from its list with the `Scan` methods in `decode.rs` (`require`, `optional`, `lists`), so that chunks it passes over are still counted.
4. Add a unit test that builds the chunk by hand in `tests/unit/`, check that no fixture snapshot changed by accident (`cargo test -p motion-import --test fixtures`), and run the local sweep: `MOTION_AEP_SAMPLES=<folder> cargo test -p motion-import -- --ignored --nocapture`. Its summed report shows how often the new part appears.

The reader must never panic or read out of bounds on any input. The property tests in `tests/unit/rifx.rs` and `tests/fixtures.rs` check this on random, truncated and corrupted files.
