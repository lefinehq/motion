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

`motion import project.aep --out scene.json` maps the decoded project to an ordinary scene ([plan](../plans/2026-11-14-import-mapping.md)) and prints what it mapped, left at its default, ignored and could not support, with where. The tables in `crates/motion-import/src/map/rows.rs` say, for every group that becomes a scene item, where each property goes, how its value converts and what it is when the project leaves it out; supporting a property is adding a row. The scene is read back with `parse_document` before it is written, so an import never yields a scene the format rejects. It then lays out every text layer it wrote and holds each line against the line the project stored for it (start, baseline, width, characters, and advances where stored), and prints how far off each text is ([D35](decisions.md#d35-fidelity-is-measured-against-reference-renders)).

- Transform defaults follow the project's: position at the composition's centre, anchor at the source's centre (0 without a source), scale 1, rotation 0, opacity 1. An effect parameter's default comes from the project's own effect definitions. An effect stores its points in fractions of its layer's effect space: its source's box, else a box the size of its composition. A layer with a source has its origin at that box's top left; a shape or text layer, whose content is drawn about its own origin, has it in the box's middle, so the import measures a point from there (a ramp on a shape layer at `[0.5, 0]` starts at `[0, -540]` in a 1920 by 1080 composition). A collapsed composition, which the import writes as `collapse`, measures them in the box of the composition it sits in, from its top left. The expression bake gives expressions the box's pixels, as the original does, and the import turns what they give into the layer's.
- Effects: the transform effect becomes `transform`, its uniform scale or its two scales as the project sets them (skew is reported). CC Particle Systems II becomes `particles`, its units turned into px and seconds by factors read off the corpus; textured or line particles, other animations, colour maps and inherited velocity are reported. Displacement Map becomes `displacement-map`, its map layer named by the id the project keeps beside the parameter (`tdpi`). Motion Tile, Offset and Turbulent Displace become `motion-tile`, `wrap` and `turbulent-displace`; a cycled evolution, a resized layer, or pinning only one edge is reported. Gradient Ramp, 4-Color Gradient, Grid, Venetian Blinds and Fractal Noise become `ramp`, `four-color`, `grid`, `venetian-blinds` and `fractal-noise`, their points in px of the layer; a scatter, jitter, feather, perspective offset or cycled evolution is reported, and so is a blending mode other than none or normal. Gaussian Blur and Fast Blur become `gaussian-blur`, Fast Box Blur `box-blur`, and Drop Shadow, Glow and Sharpen the effects of the same names; a glow based on alpha, with another operation than add or with its own colours is reported. Tint, Black & White, Hue/Saturation, Exposure, Levels and Invert become the colour effects of the same names. Curves become `curves` from the points the project keeps beside the effect's tables (`aRbp`), which the scene's spline reproduces level for level; animated curves keep their first and are reported. Pseudo effects, made only of controls, are expression controls. An effect that draws something sparse in place of what its layer draws (particles, beams, grids, audio shapes) and that the scene cannot draw yet leaves its layer drawing nothing, reported as `layers whose `<effect>` draws in their place (drawn as nothing)`, instead of the solid it would have covered.
- Keys are written in the layer's own seconds, eases as `speed(…)` with the project's speeds and influences, a linear side reaching nowhere into its span, and spatial tangents as the key's `path`.
- A value outside the range its property takes is clamped into it, as the original draws it: an opacity a wiggle drives past 100%, a negative roundness, a colour channel above 1 (a project that computes in floating point keeps such a colour, which the scene cannot hold yet). What an expression gives is clamped at each frame before it is fitted into keys, so the frames match. A stored key is clamped as a key; where the motion leaving or reaching it is a curve, the original interpolates first and clamps after, so the frames between can differ, and the report says so. Each clamp is reported under `clamped` with where.
- Properties marked hidden (3D-only on a 2D layer, the unused half of a separable position) are ignored. Expressions are reported and the keys under them kept, until they are evaluated.
- An expression whose whole body reads one control, `thisComp.layer("Controls").effect("Accent")("Color")`, `comp("Main").layer(…)…` or `effect("Size")(1)`, becomes a reference to a slot ([D44](decisions.md#d44-slots-are-named-values-that-layers-follow)) when the value it reads never changes. The slot is declared in the composition that reads its own control, or on the scene when the control is in the composition imported as the scene; a read of another composition stays baked. The slot holds the value most of the control's reads gave, in the scene's units for their property, so the scene draws as it did with the values written out; a read in other units, such as an opacity in percent beside rotations, keeps its baked value. Slot ids come from the control's name, `<layer>-<control>` for a control on a layer that has other effects, and are unique over the scene. A layer of controls only that draws nothing (a null, a guide, an adjustment or a switched-off layer) and that no layer has as parent or matte is left out, and the report says so. Strings the scene would read as references, such as a text `$name`, are written escaped, `$$name`.
- Roles are proposed ([D50](decisions.md#d50-roles-name-what-a-masters-layers-do)): a layer named `@headline Big Title` takes the role `headline` and the id `big-title`, for the roles `headline`, `subhead`, `body`, `cta`, `logo`, `product`, `background`, `price` and `date`. Otherwise a role takes the one layer, at any depth, whose id has a part that is one of its words (`title`, `logo`, `bg`, …); two such layers propose nothing. The report lists each proposal, `roles, to confirm: headline title (from its name)`, and an edit confirms or changes it, `scene roles.headline=card/title`. A master with a logo gets the default locks, `["@logo","palette","typefaces"]` ([D51](decisions.md#d51-locks-are-refused-in-the-edit)), and the report says so.
- Still footage (`.png`, `.jpg`, `.jpeg` that last no time) becomes an image layer filling the footage's size ([D41](decisions.md#d41-image-layers-name-their-file-and-carry-their-box)). The import reads no files, so the engine finds them first: a stored path that exists is taken as it is, else the file of the same name under the project's folder (eight folders deep) whose path ends with the most of the stored path's folders. `motion import` names it relative to the scene it writes. Footage it cannot find, image sequences, audio and other formats are reported under unsupported with why, such as `image footage it cannot find (drawn as nothing)`, at the layer's place.
- Video footage found the same way becomes a video layer filling the footage's size ([D47](decisions.md#d47-video-layers-play-their-clip-in-their-own-time)) when its codec decodes. The engine reads each found clip far enough to know its codec and hands that to the import with the paths, since the import reads no files. A clip in another codec is reported by it, `video footage in H.264 (drawn as nothing)`, so the report says what each decoder would unlock; a file that is not an MP4 or QuickTime clip stays `video footage (drawn as nothing)`.
- A layer the scene cannot draw yet (footage it cannot find or draw, cameras, lights, adjustment layers) becomes a `pivot`, so its children keep their parent.
- A font the import does not have (the `--font` files and folders, any `--fetch-fonts` locked, and the built-in Inter) is replaced by the closest face it has: one of the same family if any, else Inter, at the same slant and the nearest weight. A missing font's weight and slant come from its PostScript name's style suffix, through the table in `crates/motion-text/src/fallback.rs`. Each use is reported under unsupported, such as `missing fonts, substituted (`Brand-Bold → InterVariable-Bold`)`, with where ([D40](decisions.md#d40-missing-fonts-are-substituted-at-import)).

### `.mogrt` templates

`motion import template.mogrt --out scene.json` opens the archive ([D53](decisions.md#d53-a-mogrt-imports-as-its-project-with-its-controls-as-slots), [plan](../plans/2026-11-19-mogrt.md)), reads the project inside it as above, and imports the composition the template was made from. `motion inspect` prints that project.

- **The archive.** The reader reads only `definition.json`, the panel, and the project in the nested `.aegraphic` archive. Each entry is capped at 1 GiB inflated. A template with no project inside, such as one made in an editing app, is refused with why.
- **Controls become slots.** Each control of the panel becomes a slot of the scene, in the panel's order:
  - a text control becomes `text`;
  - a colour control becomes `color`;
  - a slider, angle or checkbox becomes `number`;
  - a point or scale becomes `point`.
- **Ids and labels.** The slot's id comes from the control's name, and its label is that name, `Group / Name` inside a group.
- **What follows the slot.** The project names each control's target by composition, layer and match-name path.
  - A still property the control drives follows the slot. So does a transform value the project leaves out at its default, which the import finds by its layer.
  - An expression control merges with the slot its plain reads already made, whichever composition reads it.
  - A control nothing in the scene follows, because its expressions do more than read it, is left out. The report says so: `template: 9 controls, 3 slots`, then `left out: <name> (why)`.
- **Roles.** A role of text (`headline`, `subhead`, `body`, `cta`, `price`, `date`) whose words only one text control's name has is proposed on that slot, `headline $title (from its control's name)`. This comes after a layer named for a role and before a layer id that matches.
- **Export.** `motion export` refuses a `.mogrt` as the project to write into until writing a template back has a reason.

The report has four counts per match name (mapped, defaulted, ignored, unsupported), the reasons things were ignored, the values clamped into range, and every unsupported feature with where it was met, substituted fonts among them.

## How to add a decoder

1. Write the record as a `binrw` struct in `crates/motion-import/src/record.rs`, with each field at its offset (`#[br(seek_before = at(0x..))]`) and a doc comment linking the section of the format notes it comes from. Read it with `parse`, which is the only place a `binrw` error becomes a `Diagnostic`.
2. If the record is the body of a property, add a row to `BODIES` in `crates/motion-import/src/decode/property.rs`. A wrapper that holds its stream's values next to the stream (paths, orientations, text documents) gets a `Wrapper` that lists them, one per keyframe; to keep a body opaque, give it `Wrapper::Opaque` with a reason from `Reason` in `model.rs`. Codes that become enums go through the `coded!` macro, so a new code is a new variant, not a new branch. A new text-document key is a field on its record in `decode/text.rs`, named by `#[serde(rename = "..")]`.
3. Read it from its list with the `Scan` methods in `decode.rs` (`require`, `optional`, `lists`), so that chunks it passes over are still counted.
4. Add a unit test that builds the chunk by hand in `tests/unit/`, check that no fixture snapshot changed by accident (`cargo test -p motion-import --test fixtures`), and run the local sweep: `MOTION_AEP_SAMPLES=<folder> cargo test -p motion-import -- --ignored --nocapture`. Its summed report shows how often the new part appears.

The reader must never panic or read out of bounds on any input. The property tests in `tests/unit/rifx.rs`, `tests/unit/cos.rs`, `tests/unit/decode/text.rs` and `tests/fixtures.rs` check this on random, truncated and corrupted files and text-document bodies.
