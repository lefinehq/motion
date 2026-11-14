# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Cargo workspace, repository documents and CI.
- `motion-format`: time, colour and size shorthand parsing, and `fmt_num`.
- First slice: scene format, check, at, edit.
- `motion-scene`: compile a scene into a graph of laid-out layers.
- Evaluate a scene graph at any frame.
- `motion-query`: `check` and `at` over a scene graph.
- `motion-edit`: a one-line edit language for scenes.
- `motion-cli`: the `motion` tool with `check`, `at` and `edit` commands.
- Curves as a table of names plus `cubic(x1,y1,x2,y2)` and `hold`, usable on any property.
- A property registry (`x`, `y`, `opacity`, `scale`, `rotate`) with a default and a range for each.
- Presets as data (`fade`, `fade-up`, `fade-down`, `fade-left`, `fade-right`, `pop`, `spin-in`), and a scene `presets` key to add or override them.
- `anim` tracks, as a one-line string or a keyframe list, with a curve per key; `anim` and `in` may not animate the same property.
- `scale` and `rotate` are evaluated, and `check` measures the transformed bounding box.
- Nested edit paths (`card anim.opacity=...`), JSON values in edits, and removal of objects left empty by `none`.
- Tooling: `proptest` parser tests, `insta` snapshots, coverage and `cargo-deny`.
- One error type, `Diagnostic`, with a kind (syntax, unknown name, out of range, missing, conflict, wrong type, invalid, io) that sets the exit code.
- `motion-engine`: a `Session` that is the one entry point for tools, and `motion-cli` now goes through it.
- Logs with `--log <level>`, `--log-format json` and `MOTION_LOG`, and step timings with `--timings`.
- `motion-import`: reads `.aep` projects (items, compositions, footage, layers, the property tree, keyframes, expressions and paths) into a typed model, keeps what it cannot decode yet as opaque values with a reason, and counts everything in an import report.
- `motion inspect <project.aep>` prints a project as a tree with its import report, or both as JSON with `--json`; the engine reads projects with `motion_engine::import`.
- `motion-import` decodes text documents (text, paragraph and character runs with resolved style sheets, point or box frame, and the line layout the project stored) with a `winnow` grammar for their object syntax and serde records over it, and 3D orientation angles; neither is opaque any more.
- `motion_format::grammar::parse_at`, which also hands the failing offset to the explanation; `parse_all` now takes any `winnow` input, bytes included.
- Typed property values (number, point, colour) and the transform set `position` (components `x`, `y`), `anchor`, `scale` (one number or `[x, y]`), `rotate` and `opacity`; any property can rest at a value set by its name on the layer.
- Keys ease in value space: `speed(out,out-reach,in,in-reach)` holds any ease, including a span between equal keys; a point that moves in space follows a bézier path (`"path"` on an object key) at a pace measured along it; other points can take one curve per component.
- Layers can follow a `parent`, show only in their `span`, and run on their own time with `offset` and `stretch`.
- Shape layers: `shapes` lists items (`group`, `rect`, `ellipse`, `path`, `fill`, `stroke`), each written as an object keyed by its kind with its own property registry, settings (fill `rule`, stroke `cap` and `join`), `anim` tracks, optional `id` and `hidden`. Paints cover the outlines before them in their group, through nested groups' transforms; `motion-scene` evaluates them into draws, back to front, and measures their bounds.
- Path values: SVG path data (`M`, `L`, `C`, `Z`) as a property type that tracks morph vertex by vertex.
- Effects: an `fx` list on any layer, in the same item form; `fill` is the first effect.
- Compositions: a scene `comps` map with its own size, clock, length and background, drawn by `comp` layers at the layer's own time; cycles are rejected. `pivot` layers draw nothing and carry a transform for children. Layers can be `hidden`.
- The canonical writer puts each composition's layers one per line too.
- `motion-raster`: draws a frame's display list (layers with transform, opacity, blend mode, clip and effects; filled and stroked paths; glyph runs) into an RGBA image with `vello_cpu`, and writes PNG files. Seventeen blend modes, caps, joins and fill rules are tables; the `fill` effect is the first row of the effect registry. Golden images are drawn with portable scalar code so they compare exactly across machines.
- `motion-text`: a font set read from given files and folders (by PostScript name, or family, weight and slant; every named instance of a variable font; Inter built in), and text layout with Parley and HarfRust: paragraphs, character runs, point and box text, every justification, indents, paragraph spacing, leading, tracking, baseline shift and capitals, into lines with cumulative character advances and glyph runs ready for `vello_cpu`. A missing font or glyph is an error.

### Changed

- Six crates instead of ten: the error type and shared grammar pieces live in `motion-format`, evaluation in `motion-scene`, and logging setup in `motion-engine`.
- `check` evaluates one frame at a time instead of keeping every frame in memory.
- Error messages name the path and the accepted form, with an example, for every syntax and unknown-name error.
- Shorthands (size, time, colour, rect, font, position, curves, `in`, `anim` tracks) are read by `winnow` grammars.
- The scene is read with a serde document, so errors in its shape carry the path of the value.
- Edit paths use a real grammar, so `card anim.opacity="0 -> 1 @0-12f"` and JSON values (`anim={...}`) are read by `serde_json`.
- Quiet by default: nothing is logged unless `--log` or `MOTION_LOG` asks for it, and a failing run prints only its `error:` line.

- `at` prints only the properties that differ from their default after `x y w h`.
- The property `x` and `y` are now the components of `position`; `scale` is a point (`scale=0.8,0.8` in `at`); `at` places the layer's anchor, which rests at the centre of its box, so existing scenes draw as before.
- `check` fix hints skip axes driven by `anim`.
- Properties live in one registry per kind of object (`LAYER`, `GROUP`, and one per shape item and effect); unknown-property errors name the kind (`unknown layer property`).
- A layer with two content keys reports the conflict at the second key.
- `NodeState` carries what a node draws (`content`, `local_bounds`, `fill`, `fx`, `hidden`); `check` ignores hidden layers.
- Unit tests moved out of `src/` into `tests/unit`, mirroring the source tree.
