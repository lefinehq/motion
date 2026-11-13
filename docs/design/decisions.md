Status: Draft · 2026-11-13

# Decisions

This is the record of every design decision: what we chose, and why. Other documents link here instead of repeating the reasoning. To change a decision, add a new entry that supersedes the old one. Do not edit the old entry.

## Set by the maintainers

These came from the project's direction, not from analysis.

- **motion makes video.** It is not an interactive animation runtime.
- **The pilot is an AI.** The aim is a professional-grade compositing engine for motion design, driven by an agent.
- **The tools to beat are HyperFrames and Remotion,** on tokens and on render speed.
- **No editor in v1.** Anything an agent can do must be easy to offer in an editor later.
- **3D is in scope.**
- **The repository is public** under the `lefinehq` GitHub organisation and must read as a serious, standalone project.

## Approach

We considered three ways to build motion.

**A. Our own format, checks and compositor, with an existing open-source rasteriser.** *Chosen.*
motion's value lies in the parts the agent touches: the format, the edits and the checks. We build those ourselves and let a proven library turn shapes into pixels.

**B. Build on the Rive runtime.** *Rejected.*
- It is designed for files made in Rive's editor, and for interactive use.
- It has no general blur.

**C. Build everything, including the rasteriser.** *Rejected.*
- A high-quality rasteriser takes years.
- It is not where motion's value lies.

## Design decisions

### D1. 3D in two steps

- **Decision:** 2.5D in v1; true 3D in phase 2. The scene structure supports 3D from day one.
- **Why:** 2.5D covers most motion-graphics 3D at a fraction of the cost. A 3D-ready structure means phase 2 needs no rewrite.

### D2. Vello as the rasteriser

- **Decision:** `vello_cpu` first, behind a small interface.
- **Why:**
  - It was tested by hand on this project: every effect we need in v1 works, at about 6.5 ms per 1080p frame.
  - It is pure Rust, so it ships as a single binary.
- **Rejected:**
  - **Skia:** it has the broadest effects, but a heavy C++ dependency of 20–40 MB, and its GPU backend is mid-migration.
  - **ThorVG:** its text is weak. It stays a candidate for Lottie import.

### D3. CPU-first compositor

- **Decision:** effects, masks and mattes run on the CPU in v1. GPU acceleration is phase 2, unless it is needed earlier to hit the speed goal; that is decided at M2.
- **Why:**
  - one implementation
  - bit-identical frames
  - no GPU start-up delay
  - the same behaviour on servers without a GPU

### D4. Rust core, shipped as one binary

- **Decision:** the engine is written in Rust and distributed through npm and crates.io as prebuilt binaries.
- **Why:** installing motion should not need a browser or a compiler.

### D5. JSON with short value strings

- **Decision:** scenes are JSON with CSS-like short strings. There is no custom language.
- **Why:**
  - Measured: a custom language saved only 3% of tokens.
  - JSON is the format models write most reliably.
  - Its tooling already exists.

### D6. One edit language

- **Decision:** all changes are one-line, id-based edits. JSON Patch is the fallback.
- **Why:**
  - Measured: 9 tokens for an edit, against 29 for JSON Patch and hundreds for a rewrite.
  - A future editor reuses the same commands.

### D7. motion does not call models

- **Decision:** motion ships a command-line tool, a library, an MCP server and `motion prompt`. Any agent drives it.
- **Why:**
  - It keeps the engine neutral.
  - It works with every model.
  - It has no API costs of its own.

### D8. Basic audio in v1

- **Decision:** audio files can be placed, trimmed, faded and set in volume, then mixed into the output.
- **Why:** a video tool without sound is not finished, and basic mixing is cheap.

### D9. Licence

- **Decision:** MIT OR Apache-2.0.
- **Why:** it is the Rust ecosystem's convention, and it matches Vello.

### D10. Modifiers instead of scripts

- **Decision:** scenes use declarative modifiers (jitter, loop, link, follow) instead of arbitrary code.
- **Why:** code would break the checks and repeatability.

### D11. A two-level rules prompt

- **Decision:** a core prompt of ≤ 1,000 tokens, plus topic pages of ≤ 400 tokens each, loaded only when a scene needs them.
- **Why:** the full feature set cannot fit in 1,000 tokens, and most scenes need only the core.

### D12. What "repeatable" means

- **Decision:** on the CPU path, frames are bit-identical for the same scene, version, OS and CPU type. Encoded files are not covered.
- **Why:** hardware encoders and GPUs are not deterministic.

### D13. Codec licence policy

- **Decision:**
  - no GPL code and no patent-encumbered software encoders in the binary
  - H.264 and HEVC from the operating system or GPU, or from Cisco's OpenH264 downloaded on first use
  - AV1, VP9 and ProRes always available
- **Why:** this keeps the permissive licence clean and avoids patent exposure.

### D14. No network while rendering

- **Decision:** Inter is built in. Other fonts are downloaded only through an explicit command.
- **Why:**
  - renders are repeatable and work offline
  - fonts are licensed cleanly

### D15. WebAssembly-ready core

- **Decision:** the parts that do not draw pixels stay WebAssembly-compatible.
- **Why:** a browser editor or preview can reuse them later at little cost.

### D16. Quality gates

- **Decision:**
  - text is checked against HarfBuzz glyph positions
  - overall look is signed off by the lead motion designer once per milestone, then protected by golden images
- **Why:** text correctness can be measured, but taste needs a person.

### D17. Staged releases

- **Decision:** 0.1 after M1, 0.2 after M2, and 1.0 after the agent proof.
- **Why:**
  - It gives early users something useful.
  - It gives the project a visible track record.

### D18. Naming rule

- **Decision:** no third-party product names or trademarks in the API, feature names, presets, docs or marketing. Our vocabulary is our own: nested comp, pivot, filter layer, matte, draw-on, jitter. Other tools are named only in factual, reproducible benchmark comparisons, and Lottie only as an import format. Desktop project files are referred to by their file extension (`.aep`).
- **Why:** it avoids trademark trouble, and motion should stand on its own terms.

### D19. Content rule

- **Decision:** the public repository contains no internal product names, no customer data and no links to private repositories.
- **Why:** the repository is public.

### D20. Quality is measured in five layers

- **Decision:** gates, fidelity tests, craft lint, requirement checks, and blind human votes. Everything except taste is automated; taste is judged by people only. The lead motion designer owns presets, reference clips and thresholds. Independent raters judge blind tests. Full plan in [Quality](quality.md).
- **Why:**
  - No existing metric is validated on motion graphics.
  - Pairwise human votes are the most reliable signal per unit of effort.
  - A board run by motion is only credible if competitors can rerun it.

### D21. Linear-light float compositing

- **Decision:** layers are blended in linear light at 16-bit float precision, and output is dithered. Motion blur is true motion blur, and positions are sub-pixel.
- **Why:** this is where browser-based rendering loses quality: muddy glows and blurs from 8-bit gamma-space blending, banding in gradients, and jitter in slow moves. Building it in from the start costs little and costs agents no tokens.

### D22. No AI judge

- **Decision:** no vision-language model scores clips, rankings or quality claims.
- **Why:** AI judges agree with people only about 75–83% of the time on motion graphics. They miss fine detail, and they carry biases of position, length and model family. Quality claims rest on measurable checks and human votes.

### D23. v1 is variant-first

- **Decision:** v1 turns designed projects into variants. Creating videos from scratch gets its own roadmap afterwards.
- **Why:**
  - The master is made by a professional, so quality starts high.
  - Variants are where most of the repetitive work is.
  - Real projects give exact ground truth for testing.

### D24. Import through an exporter inside the desktop tool

- **Decision:** an exporter runs inside the desktop tool and uses its documented scripting API. A direct `.aep` reader comes later, after a legal review.
- **Why:**
  - The tool reports every value, so the export is complete.
  - Using the public API carries little legal risk.
  - The tool's own render becomes ground truth for fidelity tests.

### D25. Variants work on a variant surface

- **Decision:** agents see and edit a compact summary of the master (roles, beats, editable values, style profile and locks), not the raw keyframes.
- **Why:**
  - It keeps tokens low.
  - Locked elements can't be touched by accident.

### D26. Reinterpretation stays inside the master's style

- **Decision:** when the AI reinterprets a master, it may only use easings, durations, staggers, transitions, colours and type from the master's style profile.
- **Why:** variants then move the way the designer moves. A style check makes this measurable.

### D27. One generic property model

- **Decision:** every animatable value is a registered property with a keyframe track. Curves, presets and shorthands are data that expand into tracks.
- **Why:**
  - Masters can use any curve on any property.
  - Adding a preset is a data change, not a code change. Adding a property is a registry row for parsing, `anim`, presets, edits, `at` output and clamping. A property with a geometric or visual effect also needs code in `NodeState::bounds()` and the renderer.
  - Evaluation, checks and the future renderer see one model.

## Open questions

### Q1. Name and packages (deferred)

The npm package `motion` and the Rust crate `motion` are taken, and "Motion" is another company's brand (motion.dev).

**Working answer:**
- The repository is `lefinehq/motion`.
- The npm package is `@lefine/motion`. The `@lefine` scope already exists, so the name is safe.
- No Rust crates are published in v1.

The brand can be revisited before 1.0.

### Q2. Which 3D

D1 covers 2.5D in v1: card flips, perspective, camera moves, parallax. Is that the 3D the maintainers have in mind for v1, or is true 3D (models, lights) needed earlier?
