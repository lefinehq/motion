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
      ADBE Orientation = [0, 0, 0]
report
  items: composition 1
  layers: shape 1
  effects: none
  opaque: none
  skipped: LIST:CLay 3, LIST:CPPl 1, ...
```

A text layer's document prints as its text, line count, box, and the style it starts with:

```
ADBE Text Document = text "Keep\nyour life\nsimple" (3 lines), Lato-Black 55, fill [1, 1, 1, 1], center
```

`motion inspect --json` prints the same model and report as JSON.

## What is decoded

| Part | Read from |
|---|---|
| File version and colour depth | `head`, `nhed` or `nnhd` |
| Folders, compositions and footage, with their folder | `LIST:Fold`, `LIST:Item`, `idta`, `cdta` |
| Footage: solids (colour), files (path), placeholders; size, rate, duration | `LIST:Pin `, `sspc`, `opti`, `LIST:Als2` |
| Layers: name, kind, source, parent, timing in composition seconds, switches, blend mode, track matte | `LIST:Layr`, `ldta` |
| The property tree: groups, effects, masks, names; enabled, hidden (does not apply to the layer as it is) and separated dimensions | `LIST:tdgp`, `tdmn`, `tdsn`, `tdsb`, `LIST:sspc` |
| Effect definitions: each parameter's control, default and bounds | `LIST:EfdG`, `EfDC`, `LIST:EfDf`, `LIST:parT`, `parn`, `pard`, `pdnm`, `fnam` |
| Numbers, vectors and colours, static or keyframed with interpolation, eases and spatial tangents | `LIST:tdbs`, `tdb4`, `cdat`, `LIST:list`, `lhd3`, `ldat` |
| Expressions, ranges | `Utf8` or `expr`, `tdum`, `tduM` |
| Mask and shape paths, static or keyframed | `LIST:om-s`, `LIST:omks`, `LIST:shap`, `shph` |
| 3D orientation angles, static or keyframed | `LIST:otst`, `LIST:otky`, `otda` |
| Text documents, static or one per keyframe | `LIST:btds`, `LIST:btdk` |

Names and expressions are read as UTF-8 inside `Utf8` from file version 92, and as raw, NUL-terminated text before it.

### Text documents

A text document's body is not a chunk list but a PDF-style object syntax: dictionaries, arrays, names, numbers and strings, with numbered keys. `crates/motion-import/src/cos.rs` reads it with a `winnow` grammar into a tree, and `cos/de.rs` lets serde read typed records out of that tree, so each record in `decode/text.rs` is the table of the keys it uses (`#[serde(rename = "53")] fill`).

| Part of a `TextDocument` | Keys |
|---|---|
| The text, paragraphs separated by `\n` | `/1/1/i/0/0` |
| Paragraph runs: justification, indents, spacing, leading type, hyphenation, composer | `/1/1/i/0/5` |
| Character runs: font (PostScript name), size, faux styles, leading, scales, tracking, baseline shift, kerning, caps, super- and subscript, ligatures, tsume, no-break, fill and stroke colours, stroke width and join | `/1/1/i/0/6`, fonts at `/0/1/0` |
| Point or box text, with the box | `/0/8/0`, `/1/1/i/1/0` |
| The layout the project stored: each line's origin and baseline, each segment's characters, origin, width, advances and glyph ids | `/1/1/i/1/2` |

Run ranges are byte ranges into the text, converted from the file's UTF-16 counts. A run stores only the keys that differ from its style sheet, and a sheet only the keys that differ from its parent sheet, so each run's style is resolved before it is read: the document's default style (`/1/2`, `/1/3`), then each sheet from the root down, then the run.

The stored layout is what the project last drew, so our own text layout can be measured against it. The segment origins, widths and glyph ids are inferred from the samples; per-character advances are read where a file stores them, which none of the local samples do.

## What is opaque

A property whose value the reader keeps but cannot read yet is `opaque` with one of these reasons:

| Reason | What it is |
|---|---|
| `gradient` | Gradient colours (`LIST:GCst`) |
| `markers` | Layer markers (`LIST:mrst`) |
| `valueless` | A stream with no value, such as a layer reference |
| `dimensions` | A stream whose dimension count the reader does not handle |
| `other_body` | A property body not in the table |

Chunks outside the property tree that the reader does not need yet (views, render settings, guides, caches) are passed over and listed under `skipped`. Across the 37 local sample projects, 674 properties are keyframed, 11,371 are static, and 28 are opaque (17 markers, 5 gradients, 6 other bodies), down from 613 before orientations (538) and the 47 text documents were decoded.

## Importing as a scene

`motion import project.aep --out scene.json` maps the decoded project to an ordinary scene ([plan](../plans/2026-11-14-import-mapping.md)) and prints what it mapped, left at its default, ignored and could not support, with where. The tables in `crates/motion-import/src/map/rows.rs` say, for every group that becomes a scene item, where each property goes, how its value converts and what it is when the project leaves it out; supporting a property is adding a row. The scene is read back with `parse_document` before it is written, so an import never yields a scene the format rejects.

- Transform defaults follow the project's: position at the composition's centre, anchor at the source's centre (0 without a source), scale 1, rotation 0, opacity 1. An effect parameter's default comes from the project's own effect definitions.
- Keys are written in the layer's own seconds, eases as `speed(…)` with the project's speeds and influences, a linear side reaching nowhere into its span, and spatial tangents as the key's `path`.
- Properties marked hidden (3D-only on a 2D layer, the unused half of a separable position) are ignored. Expressions are reported and the keys under them kept, until they are evaluated.
- A layer the scene cannot draw yet (footage files, cameras, lights, adjustment layers) becomes a `pivot`, so its children keep their parent.

## How to add a decoder

1. Write the record as a `binrw` struct in `crates/motion-import/src/record.rs`, with each field at its offset (`#[br(seek_before = at(0x..))]`) and a doc comment linking the section of the format notes it comes from. Read it with `parse`, which is the only place a `binrw` error becomes a `Diagnostic`.
2. If the record is the body of a property, add a row to `BODIES` in `crates/motion-import/src/decode/property.rs`. A wrapper that holds its stream's values next to the stream (paths, orientations, text documents) gets a `Wrapper` that lists them, one per keyframe; to keep a body opaque, give it `Wrapper::Opaque` with a reason from `Reason` in `model.rs`. Codes that become enums go through the `coded!` macro, so a new code is a new variant, not a new branch. A new text-document key is a field on its record in `decode/text.rs`, named by `#[serde(rename = "..")]`.
3. Read it from its list with the `Scan` methods in `decode.rs` (`require`, `optional`, `lists`), so that chunks it passes over are still counted.
4. Add a unit test that builds the chunk by hand in `tests/unit/`, check that no fixture snapshot changed by accident (`cargo test -p motion-import --test fixtures`), and run the local sweep: `MOTION_AEP_SAMPLES=<folder> cargo test -p motion-import -- --ignored --nocapture`. Its summed report shows how often the new part appears.

The reader must never panic or read out of bounds on any input. The property tests in `tests/unit/rifx.rs`, `tests/unit/cos.rs`, `tests/unit/decode/text.rs` and `tests/fixtures.rs` check this on random, truncated and corrupted files and text-document bodies.
