# motion documentation

motion is an open-source engine for making motion graphics and video. It is designed to be driven by an AI agent. These documents explain what we are building and why.

## Start here

Read these in order. Each one takes about five minutes.

1. [Overview](design/overview.md): what motion is, who it is for, and what we are aiming for.
2. [Scope](design/scope.md): what is in v1, what comes later, and what motion will not do.
3. [Scene format](design/scene-format.md): how a video is described.
4. [Edits and checks](design/edits-and-checks.md): how an agent changes a scene and checks it without watching it.
5. [Rendering](design/rendering.md): how a scene becomes a video file, and how we keep that fast.
6. [Features](design/features.md): what motion can do in v1, and what comes later.
7. [Variants](design/variants.md): how a designed project becomes many professional variants (the v1 focus).
8. [Quality](design/quality.md): how we judge clips and run a fair public scoreboard.
9. [Roadmap](design/roadmap.md): milestones, releases and risks.
10. [Decisions](design/decisions.md): every design decision, with its reason.

Reference:

- [Glossary](glossary.md): the words we use and what they mean.
- [`.aep` import](design/aep-import.md): what the project reader decodes, what it keeps opaque, and how to add a decoder.
- [Research: baselines, 2026-11-13](research/2026-11-13-baselines.md): the measurements behind our goals.
- [Research: quality measurement, 2026-11-13](research/2026-11-13-quality-measurement.md): how quality is measured elsewhere, and what we should adopt.
- [Research: fidelity of the first sample, 2026-11-14](research/2026-11-14-fidelity-first-sample.md): `motion diff` against a reference render, and what the numbers show.

## How we write docs

Every document in this folder follows these rules.

- **One topic per file.** If a document needs a table of contents, split it.
- **Lead with the point.** The first two or three sentences say what the document covers and the one thing to remember.
- **Example first, rules second.** Show a scene, a command or an output before explaining it.
- **Plain words.** Define a term the first time it appears, or link to the [glossary](glossary.md).
- **Every number has a source.** Mark it as *measured*, *target* or *estimate*. Measured numbers link to the research note that produced them.
- **Decisions live in one place.** [decisions.md](design/decisions.md) holds each decision with a number, a date and a reason. Other documents link to it (for example "see D5") instead of repeating the argument.
- **A status line on top.** Each document starts with `Status: Draft`, `Accepted` or `Superseded`, and a date.
- **Our own vocabulary.** We do not use other products' names or trademarks for features. Other tools are named only in factual, reproducible benchmark comparisons (see [D18](design/decisions.md#d18-naming-rule)).
