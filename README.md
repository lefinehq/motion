# motion

**An open-source, AI-native engine for programmable motion graphics and video.**

You describe a video as a small scene file; motion checks it, edits it and renders it to a finished video, with no browser and nothing else to install.

![A scene file, the render command, and the video motion made from it](docs/media/demo.gif)

*Made by motion itself. A 290-byte scene, one command: 5 seconds of 1080p H.264 in 0.46 s on an Apple M4 Max.*

**Status:** early development. There is no release to install yet; you build it from source.

## Why motion

When an AI agent makes a motion-graphics video today, it usually writes web code, and a headless browser plays that code and takes a screenshot of every frame. That works, but the agent pays for it: long scene code, whole rewrites for small changes, frames it has to look at to know whether anything is wrong, and a browser to start for every render ([measured comparison](docs/design/overview.md#the-problem)).

motion works on the scene itself:

- **The scene is data.** A scene says what should happen ("five cards fade up, six frames apart"), not where every element is on every frame. The example above is 290 bytes.
- **Edits are small.** `motion edit scene.json 'card fill=#FF0000'` changes one value and keeps the rest of the file as it was.
- **Checks speak in numbers, not pictures.** `motion check` reports layers leaving the frame, text too faint or too brief to read, and flashing, each with the fix, so an agent never has to look at frames to find out.
- **Rendering is native.** Layers are composited in linear light at float precision, with true motion blur, and encoded straight to H.264, HEVC, ProRes or AV1.
- **Existing projects come in.** motion imports `.aep` projects and `.mogrt` templates, so work that already exists can become a programmable workflow (see the [roadmap](#compatibility-roadmap)).

## Try it in 30 seconds

After the first build (which compiles the workspace once), from the repository root:

```
cargo run -q -p motion-cli -- render examples/launch.json --out /tmp/launch.mp4
150 frames written to /tmp/launch.mp4 (H.264, web, 1920x1080, 30 fps, 5 s)

cargo run -q -p motion-cli -- check examples/launch.json
OK 4
```

Open `examples/launch.json`, change a word or a colour, and render again.

## What you can build with it

- **Existing projects → programmable workflows.** Import a designer's `.aep` project, find its text, colours and timings as named slots, and drive them from data.
- **One project → many formats and variations.** New copy, other languages, 9:16 and 1:1 cuts, shorter versions, all from one master.
- **AI agents → inspect, edit and render.** An agent reads what a scene holds (`roles`, `slots`, `at`), changes it with one-line edits, checks the result in text and renders it, without writing web code.
- **Motion design → automated production.** Batch a master into every variant it needs: `motion batch` applies each variant's edits through the master's locks, checks it, and renders those that pass.

## Compatibility roadmap

motion reads files from desktop compositing and editing tools by their file format. Fidelity is measured against reference renders, sample by sample; nothing here is complete yet.

| Format | State |
|---|---|
| `.aep` projects | **In development.** Layers, shape modifiers, text animators, masks and mattes, many effects, expressions (baked to keys), 2.5D cameras, footage, audio. Measured on 138 public templates: 7 match their reference render on every frame today (within the reference's own compression noise), 5 more once their opening settles. |
| `.mogrt` templates | **In development.** Imported like `.aep`; the template's controls become the scene's slots. |
| Edits back into `.aep` | **In development.** `motion export` patches changed values into the original project and lists what cannot go back. |
| Vector footage (`.ai`, `.pdf`, `.eps`) | **Supported**, drawn by motion's own renderer, including a built-in PostScript interpreter. |
| `.drp` timelines | **Planned.** Not started. |

What the import cannot do yet, it says: every import prints what it mapped, defaulted, ignored or could not support.

## Contributing

motion needs more than Rust code, and you are welcome whichever of these you are:

- **Rust developers:** the renderer, the import, the encoders and the checks. Start with [Architecture](ARCHITECTURE.md) and [Contributing](CONTRIBUTING.md).
- **Motion designers:** tell us where a render looks wrong, which presets and easings you reach for, and what a professional result needs. Taste is judged by people, never by a model ([D20](docs/design/decisions.md#d20-quality-is-measured-in-five-layers), [D22](docs/design/decisions.md#d22-no-ai-judge)).
- **People who work in desktop compositing and editing tools:** import your own `.aep` projects and `.mogrt` templates, and open an issue when a frame does not match. A project that renders wrong is the most useful bug report we can get.

Small, clear changes with tests are the best kind. See [Contributing](CONTRIBUTING.md) for how to send one.

## Documentation

- [Documentation index](docs/README.md): start here.
- [Overview](docs/design/overview.md): what motion is and who it is for.
- [Scene format](docs/design/scene-format.md): every key a scene can hold.
- [Rendering](docs/design/rendering.md): how a scene becomes a video file.
- [Writing a video from scratch](docs/guide/from-scratch.md): the guide for agents, also an agent skill in [`skills/motion`](skills/motion/SKILL.md), with [examples](examples/README.md).
- [Architecture](ARCHITECTURE.md): the crates and how they fit together.
- [Contributing](CONTRIBUTING.md): how to send a change.

## Building from source

You need the Rust toolchain; `rust-toolchain.toml` pins the version and `rustup` installs it for you.

```
cargo test --workspace
```

## Command reference

Build the `motion` tool and point it at the example scene:

```
cargo run -q -p motion-cli -- check examples/launch.json
OK 4

cargo run -q -p motion-cli -- at examples/launch.json 1s title
title x=960 y=260 w=580.8 h=115.2

cp examples/launch.json /tmp/launch.json
cargo run -q -p motion-cli -- edit /tmp/launch.json 'card fill=#FF0000'
OK 4

cargo run -q -p motion-cli -- render examples/launch.json --frames 0-29 --out /tmp/frames
30 frames written to /tmp/frames

cargo run -q -p motion-cli -- render examples/launch.json --out /tmp/launch.mp4
150 frames written to /tmp/launch.mp4 (H.264, web, 1920x1080, 30 fps, 5 s)
```

`render` writes `frame-00000.png` and on into a folder, one frame per processor thread at a time (`--threads N` draws on at most N, which also caps the memory a render takes); `--frame N` draws one, and with neither flag every frame is drawn. `--quality fast` trades float compositing for speed in previews.

When `--out` is a video file (`.mp4` or `.mov`), the frames are encoded in order as they are drawn, with nothing else installed. `--codec` picks the codec: `h264` and `hevc` from the system's encoder on macOS, where H.264 is the default, `prores` (ProRes 4444 with alpha, in a `.mov` file, for scenes with a clear background) on macOS, and `av1` on every platform, and `--video-quality` what the file is for: `draft` to check timing, `web` to deliver (the default), `master` to edit or encode again. `--timings` shows the `encode` step apart from drawing.

`check` exits 1 when it finds an error. `check --baseline template.json` prints only what the earlier version, such as the template before your edits, does not already have, so the exit code gates your changes alone. `edit` saves a valid scene and prints what the edit added to the check the same way, so it exits 1 only when the edit made a new error. Every command exits 2 when the file, the time or the edit line is unusable.

`import` turns an `.aep` project or a `.mogrt` template into a scene and prints what it mapped, defaulted, ignored or could not support yet; `export` writes an edited scene back into the project it came from.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT licence](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution you submit for inclusion in this project is dual licensed as above, with no additional terms or conditions.
