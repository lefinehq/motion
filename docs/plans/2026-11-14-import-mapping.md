# Plan: from an `.aep` project to a scene

**Goal:** step 7 of [the .aep-to-frames plan](2026-11-14-aep-to-frames.md). `motion import project.aep` writes an ordinary scene ([D32](../design/decisions.md#d32-one-format-for-authored-and-imported-scenes)) and a mapping report. The scene is built as JSON and read back through `parse_document`, so an import can never produce a scene the format rejects.

It ships in two pull requests: 7a reads what the mapping needs from the file, 7b maps.

## 7a. What the decoder adds

- **Flags.** A property or group whose `tdsb` marks it hidden does not apply to the layer as it is: 3D-only properties on a 2D layer (`ADBE Orientation`, `ADBE Rotate X`, the material options), and the unused half of a separable position. The mapping skips them, so a 2D layer's 3D-only streams are never reported. The `separated` flag says when `ADBE Position_0` and `_1` hold the position instead of `ADBE Position`.
- **Effect definitions.** A project stores an effect parameter only when it differs from its default, and the defaults are in the file: each effect type's parameter definitions (`pard`), with control type, default and bounds, in the project's table (`LIST:EfdG`) and in the first instance of each effect. The decoder reads them into `Project::effects`, so effect defaults come from the file, never from a table we keep. For example, the Fill effect's colour defaults to red and its opacity to 1.

## What the first sample needs

Measured on the first sample (`motion inspect`):

- Layer transforms: a project leaves out a transform property at its default. Checked against the Lottie exports of the fixtures (30 layers): position defaults to the composition's centre, anchor to the source's centre (0 for layers without a source), scale to 1, rotation to 0, opacity to 1.
- Every keyed move (the title, the squares, the hashtag, the button) runs through an `outExpo` expression from a common easing script, over linear keys. Until expressions are evaluated, the import keeps the keys and reports each expression; the motion then has the right timing but not the right ease.
- The hashtag is revealed by an inverted alpha track matte, which is reported as unsupported until mattes come with the compositor.

## Shape of the mapping

`crates/motion-import/src/map/`, one module per part, each driven by tables:

| Module | Maps |
|---|---|
| `scene.rs` | The chosen composition becomes the scene: size, fps, length, background. Every composition it reaches through layers becomes an entry of `comps`. |
| `layer.rs` | Layer kinds: shape, text, precomposition, solid (a `rect` with its colour), null (`pivot`). Ids, `parent`, `span` (in and out points), `offset` (start time), `stretch`, `hidden` (the enabled switch). |
| `property.rs` | One table from a match name to a scene target: a layer, group, shape-item, effect or animator property (or a component of one), with a conversion (`Percent` to a fraction, `Point` dropping z, `Color`, `Degrees`, `Choice` for enumerations). |
| `keys.rs` | Keyframes to `anim` key lists. Temporal eases become `speed(out, out-reach, in, in-reach)`: the speed in the property's units per second, the reach being the influence. Hold keys become `hold`. Spatial tangents become a key's `path`. |
| `shape.rs` | The shape content tree to `shapes` items, through the property table. |
| `text.rs` | Text documents to `text`, `font`, `fill`, `style`, `runs`, `paragraphs` and `box`, and text animators to `animate`. |
| `defaults` | Not a module: every row of the property table also gives the property's default in the project's units, and an effect parameter's default comes from the project's own definitions. |

The tables are data: adding a supported property is adding a row. A test walks every row and checks that its target exists in the scene format's registries.

## What it reports

The mapping report lists every match name it met, with how many times it was mapped, left at its default, or not supported, and why:

- **Unsupported:** expressions (until they are evaluated at import), 3D layers and cameras, masks, track mattes, layer styles, blend modes the renderer lacks, effects without a row.
- **Ignored, on purpose:** hidden properties and groups (see 7a), and groups that change nothing a 2D frame shows, such as audio levels and switched-off layer styles. A switched-on layer style is reported.

`motion import` prints the report, and `--json` writes it as JSON with the scene.

## 7b. What changed in the format

- A layer may set `position` itself, an exact point in its parent's own coordinates, instead of `at`, which reads the parent's box. A parent's box does not start at its origin for shapes and text, while the project's child positions do.
- `offset` and key times may be negative: projects often start a layer before its own zero (the first sample's title starts at -2.52 s).
- `box` takes `[x, y, w, h]` too, since box text in a project rarely starts at the layer's origin.

## Ids

Layer and composition names become ids: lower case, `-` for runs of other characters, and a number added when two names collide. The report lists every name with its id.

## Measures

- The first sample imports with nothing unsupported but its 10 expressions and its one track matte (100 properties mapped, 67 defaulted, 182 ignored). From 2 s on, its frames match the reference render's layout by eye; before, the reference moves faster, since its moves run through `outExpo` expressions over the linear keys.
- Every composition of every fixture imports, and reads back unchanged once written (`tests/import.rs`), with its scene and report as snapshots.
- The local sweep imports all 114 compositions of the samples: 16,116 properties mapped, 10,451 defaulted, 12,647 ignored, 353 unsupported. What is not supported, by how often it is met: expressions (2,125), trim paths (190), 3D layers (81), masks (34), track mattes (39), stars (15), layer styles (16), cameras (8), dashes (7), time remapping (7), gradients (6), and effects without a row.
- Step 8 compares its frames with the reference render, in numbers.
