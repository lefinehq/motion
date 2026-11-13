Status: Draft · 2026-11-13

# Overview

motion turns a short description of a video into a finished video file. An AI agent writes the description, motion checks it and tells the agent what is wrong in plain numbers, and then renders the video without a web browser. The aim is professional motion-design quality at a fraction of the tokens and render time that today's tools need.

## v1: variants first

v1 starts from a designer's finished project rather than a blank page.
- **Import:** the project comes in through an exporter that runs inside the desktop tool.
- **Variants:** the AI makes professional versions of it: new copy, new sizes, shorter cuts, other languages, and its own takes within the designer's style. See [Variants](variants.md).
- **Later:** creating videos from scratch gets its own roadmap once this works.

## The problem

When you ask an AI agent to make a motion-graphics video today, it usually writes web code: React components for Remotion, or HTML, CSS and GSAP for HyperFrames. A headless browser then plays that code and takes a screenshot of every frame.

That works, but an agent pays for it in four places. We measured all four on the same scene and the same machine (an Apple M4 Max). The details are in the [research note](../research/2026-11-13-baselines.md).

| What the agent pays for | HyperFrames | Remotion |
|---|---|---|
| Writing the scene | 833 tokens | 680 tokens |
| One small change ("make card 3 red and bring it in 10 frames later") | 59 tokens | 51 tokens |
| Rules to read before starting | 8,000–15,000 tokens | about 840 tokens |
| Checking the result | a text report built from 9 sampled frames | render frames and look at them: 7,000–15,000 image tokens |
| Rendering a 5-second 1080p clip | 5.3 s | 3.3 s |

All figures are measured.

Two other well-known formats show that animation can be stored as data. But neither was built for an agent:

- **Lottie** is an export format, and verbose: the same scene costs about 4,100 tokens.
- **Rive** is a closed binary format that needs its own editor.

## What motion does differently

### 1. Describe meaning, not frames

A scene says *what should happen*, not where every element is on every frame:

```json
{"id":"cards","repeat":5,"layout":"row gap 40","in":"fade-up 60px 12f ease-out @15f stagger 6f"}
```

This line says: five cards in a row, each fading up into place, starting at frame 15, six frames apart. The engine works out the frames. The whole reference scene is about **190 tokens** (*measured* on a prototype encoding).

### 2. Change one thing with one line

```
card3 fill=red delay+=10f
```

That is the whole edit: **9 tokens** (*measured*). The agent never rewrites the scene.

### 3. Ask instead of look

The agent asks motion questions and gets numbers back, not pictures:

```
E overflow cards.3.text 37-150f +18px; fix: cards.3 font.size<=34
OK 41
```

This says that card 3's text spills out of its box by 18 pixels from frame 37 to frame 150, and suggests the edit that fixes it. The other 41 checks passed. No image was rendered or viewed.

### 4. Render without a browser

motion computes each frame directly and draws it with a native vector renderer. There is no browser to start, no page to lay out and no screenshot to take. Our target for the reference clip is **under 0.5 seconds** (*target*), against Remotion's 3.3 seconds.

## Who it is for

- **AI agents and the tools built on them** that need to produce video, for example coding agents, content pipelines and chat assistants.
- **Developers** who want programmable video without running a browser.
- **Future editors.** Every change in motion is a one-line command, so a timeline editor can be built later as a simple layer on top. Anything an agent can do, an editor can do too (see [D6](decisions.md#d6-one-edit-language)).

## Goals

These goals are measured on the reference scene described in the [roadmap](roadmap.md#benchmarks).

| Goal | Target | Today's best |
|---|---|---|
| Writing the reference scene | ≤ 200 tokens | 680 (Remotion) |
| One semantic edit | ≤ 15 tokens | 51 (Remotion) |
| Core rules prompt | ≤ 1,000 tokens, plus topic pages of ≤ 400 tokens loaded only when needed | ~840 (Remotion) |
| A passing check | ≤ 50 tokens, no images | 403 (HyperFrames) |
| Rendering the 5 s reference clip | ≤ 0.5 s end to end, cold start ≤ 150 ms | 3.3 s (Remotion) |
| Text shaping | correct for Latin, CJK, Arabic, Hebrew and emoji | — |
| Repeatability | the same scene always gives bit-identical frames on the same OS and CPU type | — |
| Feature set | the full core tier in [Features](features.md) | — |
| Import coverage | ≥ 95% of layers and properties in the reference projects map exactly | — |
| A variant (for example German, 9:16, 15 s) | ≤ 300 tokens of edits, rendered faster than real time | — |
| Designer approval | ≥ 80% of variants approved without changes | — |

Two notes on the targets:

- **Render speed:** the target applies to the reference machine. A Linux server with no GPU is our second target. Its numbers will be measured and published, and it must still beat Remotion on the same server.
- **Visual quality:** frames must look at least as good as both competitors'. The maintainers judge this on a side-by-side sheet at each milestone, and golden images then lock the result in.

## What motion is not

motion is not interactive, not a browser, not a footage editor and not an AI model. It makes video files from scenes. The full in-scope, later and out-of-scope lists are in [Scope](scope.md).

## Principles

1. **Store meaning, not keyframes.** Keep intent such as "fade up, staggered", and let the engine expand it. Keyframes are available when needed, but they are the exception.
2. **Compact by default.** Sensible defaults, presets and short value strings, so a typical scene stays small.
3. **Stable, human ids.** Every element has a name people would use. Repeated items are numbered from 1, because people say "card 3".
4. **One edit language for everyone.** Agents and future editors change scenes through the same one-line commands.
5. **Ask, don't look.** Anything you would check by watching the video, you can ask motion for as numbers.
6. **Native and repeatable.** No browser. A frame depends only on the scene, the time and the motion version.
