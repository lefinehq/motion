Status: Draft · 2026-11-13

# Scope

This document says what motion does in v1, what comes later, and what it will not do. It is the single source for scope. If another document disagrees, this one wins.

## The scope test

A request is in scope only if it passes all three tests:

1. **It helps turn a scene into a video file.** motion produces video. It is not a player, an app runtime or a service.
2. **It can be written in the scene format, and checks still work on it.** If the agent can't describe it compactly, or motion can't answer questions about it, it doesn't fit.
3. **Rendering stays offline.** The render needs no browser, no network and no AI model.

## In scope for v1

### Output

- **Video:**
  - MP4 (H.264, HEVC)
  - WebM (VP9, AV1)
  - MOV ProRes 4444 with transparency
- **Images:**
  - PNG sequences
  - single stills (`motion still`)
- **Canvas:**
  - any size up to 4K, any aspect ratio
  - up to 60 fps
- **Length:** tuned for clips up to about 5 minutes. Longer clips work but are not optimised.

### Input

- **The scene:** a JSON file.
- **Fonts:**
  - Inter, built in
  - font files
  - fonts downloaded with `motion fonts add`
- **Images:** PNG, JPEG, WebP, SVG.
- **Audio:** WAV, MP3, AAC, FLAC.
- **Data and variables:** render one scene many times with different data, for example `motion render scene.json --data rows.json`. This is a common need in programmable video, and the `data` field already supports it.

### Import and variants (the v1 focus)

- **Import:** desktop project files (`.aep`) through an exporter that runs inside the tool, with a coverage report.
- **Variants:** swap, reframe, cut down, localise, and reinterpret within the master's style. See [Variants](variants.md).

### Features

Everything in the [core tier](features.md#core-tier-v1):

- layers, nested comps, pivots and layout
- the three animation levels, modifiers and text motion
- shapes and path operations
- masks, mattes and blend modes
- motion blur
- 2.5D depth with a camera
- the core effects
- basic audio
- checks

### Ways to drive motion

- **The command-line tool:** `render`, `still`, `check`, `at`, `assert`, `edit`, `explain`, `prompt`, `fonts`, `bench`.
- **An MCP server** with the same operations.
- **A Node library** with TypeScript types.
- **`motion prompt`:** the core prompt and topic pages that teach an agent.

### Platforms and languages

- **Operating systems:** macOS, Linux and Windows.
- **Scripts:** text in any script, including right-to-left scripts, CJK and emoji.

## Later (planned, not in v1)

| Item | Why it waits |
|---|---|
| Creating videos from scratch as the main workflow | v1 is variant-first; this gets its own roadmap. The scene format already supports it. |
| Reading `.aep` files without the tool | Needs a legal review first |
| Writing variants back to the tool's format | Useful for designers, not needed for v1 |
| Video layers (clips inside a scene) | Decoding and seeking video is a large piece of work of its own |
| True 3D: models, lights, materials | v1 covers 2.5D. The scene structure is ready for 3D. |
| Particles, retiming, distortion, LUTs, HDR | Extended tier |
| GPU acceleration | Only needed for heavy scenes, unless M2 shows otherwise |
| Lottie import | A route to existing animations, but not core |
| Caption import (SRT, VTT) | Captions can already be built from text layers |
| Animation driven by audio (beats, waveforms) | Needs audio analysis |
| GIF output | Low priority, and a separate encoder |
| Live preview (a player that reloads as the scene changes) | Agents don't need it, but people will |
| Browser preview or player (WebAssembly) | The core is kept ready for it (see [D15](decisions.md#d15-webassembly-ready-core)) |
| A timeline editor | Will sit on top of the edit language |
| Publishing Rust crates | Only if people ask to embed motion in Rust |

## Out of scope

These are deliberate choices, not gaps.

| motion will not… | Because |
|---|---|
| **Generate media with AI**: video from text, images, voices, music | motion arranges and animates assets. Agents bring the assets from other tools. |
| **Call AI models** or run an agent itself | motion is the engine an agent drives. It stays neutral and has no API costs (see [D7](decisions.md#d7-motion-does-not-call-models)). |
| **Be interactive**: UI animation in apps, state machines, input handling | motion makes video files |
| **Edit footage**: long-form cutting, rotoscoping, tracking, keying, stabilisation | That is a video editor's job, not a motion-design engine's |
| **Run web content**: HTML, web pages, JavaScript libraries inside scenes | It would need a browser and would break the checks |
| **Run arbitrary scripts** inside scenes | Modifiers cover the need, and stay checkable (see [D10](decisions.md#d10-modifiers-instead-of-scripts)) |
| **Support third-party effect plugins** | Effects are written for motion and checked by motion |
| **Stream live** or render in real time for broadcast | motion renders files offline |
| **Do advanced audio production**: effects, EQ, mastering | v1 places, trims, fades and mixes. That is enough for video. |
| **Be a hosted service**: accounts, cloud rendering, billing | The open-source project is the engine. Chunked rendering makes it easy for others to run it at scale. |
| **Ship an asset or template marketplace** | Out of the engine's job |
| **Provide mobile or embedded runtimes** | There is nothing to embed: the output is a video file |

## Changing scope

To move an item between these lists:

1. Add a decision to [decisions.md](decisions.md) with the reason.
2. Update this document.

Requests that fail the scope test above are closed with a link to this page.
