# Getting started

What nakkash is for, how it differs from rendering web code in a browser, and every command with an example. For a short tour, see the [README](../../README.md).

## Why nakkash

When an AI agent makes a motion-graphics video today, it usually writes web code, and a headless browser plays that code and takes a screenshot of every frame. That works, but the agent pays for it: long scene code, whole rewrites for small changes, frames it has to look at to know whether anything is wrong, and a browser to start for every render ([measured comparison](../design/overview.md#the-problem)).

nakkash works on the scene itself:

- **The scene is data.** A scene says what should happen ("five cards fade up, six frames apart"), not where every element is on every frame. The example above is 290 bytes.
- **Edits are small.** `nakkash edit scene.json 'card fill=#FF0000'` changes one value and keeps the rest of the file as it was.
- **Checks speak in numbers, not pictures.** `nakkash check` reports layers leaving the frame, text too faint or too brief to read, and flashing, each with the fix, so an agent never has to look at frames to find out.
- **Rendering is native.** Layers are composited at float precision in the blending space the project uses (linear-light blending is planned), with true motion blur, and encoded straight to H.264, HEVC, ProRes or AV1.
- **Existing projects come in.** nakkash imports `.aep` projects and `.mogrt` templates, so work that already exists can become a programmable workflow (see [compatibility](../compatibility.md)).

## What you can build with it

- **Existing projects → programmable workflows.** Import a designer's `.aep` project, find its text, colours and timings as named slots, and drive them from data.
- **One project → many formats and variations.** New copy, other languages, 9:16 and 1:1 cuts, shorter versions, all from one master.
- **AI agents → inspect, edit and render.** An agent reads what a scene holds (`roles`, `slots`, `at`), changes it with one-line edits, checks the result in text and renders it, without writing web code.
- **Motion design → automated production.** Batch a master into every variant it needs: `nakkash batch` applies each variant's edits through the master's locks, checks it, and renders those that pass. `nakkash vary` generates them instead: new palettes, faces and copy, lengths and sizes, each checked against the master.

## Try it

After the first build (which compiles the workspace once), from the repository root:

```
cargo run -q -p nakkash-cli -- render examples/launch.json --out /tmp/launch.mp4
150 frames written to /tmp/launch.mp4 (H.264, web, 1920x1080, 30 fps, 5 s)

cargo run -q -p nakkash-cli -- check examples/launch.json
OK 4
```

Open `examples/launch.json`, change a word or a colour, and render again.

## Command reference

Build the `nakkash` tool and point it at the example scene:

```
cargo run -q -p nakkash-cli -- check examples/launch.json
OK 4

cargo run -q -p nakkash-cli -- at examples/launch.json 1s title
title x=960 y=260 w=580.8 h=115.2

cp examples/launch.json /tmp/launch.json
cargo run -q -p nakkash-cli -- edit /tmp/launch.json 'card fill=#FF0000'
OK 4

cargo run -q -p nakkash-cli -- render examples/launch.json --frames 0-29 --out /tmp/frames
30 frames written to /tmp/frames

cargo run -q -p nakkash-cli -- render examples/launch.json --out /tmp/launch.mp4
150 frames written to /tmp/launch.mp4 (H.264, web, 1920x1080, 30 fps, 5 s)
```

`render` writes `frame-00000.png` and on into a folder, one frame per processor thread at a time (`--threads N` draws on at most N, which also caps the memory a render takes); `--frame N` draws one, and with neither flag every frame is drawn. `--quality fast` trades float compositing for speed in previews.

When `--out` is a video file (`.mp4` or `.mov`), the frames are encoded in order as they are drawn, with nothing else installed. `--codec` picks the codec: `h264` and `hevc` from the system's encoder on macOS, where H.264 is the default, `prores` (ProRes 4444 with alpha, in a `.mov` file, for scenes with a clear background) on macOS, and `av1` on every platform, and `--video-quality` what the file is for: `draft` to check timing, `web` to deliver (the default), `master` to edit or encode again. `--timings` shows the `encode` step apart from drawing.

`check` exits 1 when it finds an error. `check --baseline template.json` prints only what the earlier version, such as the template before your edits, does not already have, so the exit code gates your changes alone. `edit` saves a valid scene and prints what the edit added to the check the same way, so it exits 1 only when the edit made a new error. Every command exits 2 when the file, the time or the edit line is unusable.

`import` turns an `.aep` project or a `.mogrt` template into a scene and prints what it mapped, defaulted, ignored or could not support yet; `export` writes an edited scene back into the project it came from.
