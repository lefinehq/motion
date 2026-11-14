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

### Changed

- Six crates instead of ten: the error type and shared grammar pieces live in `motion-format`, evaluation in `motion-scene`, and logging setup in `motion-engine`.
- `check` evaluates one frame at a time instead of keeping every frame in memory.
- Error messages name the path and the accepted form, with an example, for every syntax and unknown-name error.
- Shorthands (size, time, colour, rect, font, position, curves, `in`, `anim` tracks) are read by `winnow` grammars.
- The scene is read with a serde document, so errors in its shape carry the path of the value.
- Edit paths use a real grammar, so `card anim.opacity="0 -> 1 @0-12f"` and JSON values (`anim={...}`) are read by `serde_json`.
- Quiet by default: nothing is logged unless `--log` or `MOTION_LOG` asks for it, and a failing run prints only its `error:` line.

- `at` prints only the properties that differ from their default after `x y w h`.
- `check` fix hints skip axes driven by `anim`.
- Unit tests moved out of `src/` into `tests/unit`, mirroring the source tree.
