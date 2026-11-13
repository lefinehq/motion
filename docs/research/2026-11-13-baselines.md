Status: Accepted · 2026-11-13

# Research: baselines and feasibility

On 2026-11-13 we ran four independent investigations before designing motion. This note records what they found. Four conclusions:

1. **Writing the scene.** Today's tools need 680–833 tokens to write a simple scene. A compact format needs about 190.
2. **Rendering.** They take 3–5 seconds to render a 5-second clip.
3. **Vello.** It can draw everything v1 needs, quickly.
4. **Prior art.** No existing project combines a compact format, checks without images, and rendering without a browser.

**Machine:** Apple M4 Max (14 CPU cores, 32-core GPU, 36 GB RAM), macOS. Token counts use the o200k tokenizer unless noted.

## Competitor baselines

We built the reference scene in HyperFrames (CLI 0.9.41) and Remotion (4.0.543), the way each is normally used, and rendered each twice.

| | HyperFrames | Remotion |
|---|---|---|
| Tokens to write the scene | 833 | 680 |
| Tokens for the standard edit | 59 | 51 |
| Rules an agent reads first | 8,000–15,000 | ~840 |
| Render, first run | 6.5 s | 12.6 s (includes an 8.5 s first build) |
| Render, second run | 5.3 s | 3.3 s |
| Frame capture alone | ~68 fps | ~114 fps |
| How the agent checks the result | `hyperframes check`: a text report from 9 sampled frames, 403 tokens, 1.6 s | render stills and view them, about 7,000–15,000 image tokens |

For short clips, start-up dominates in both tools: launching the browser and building the bundle. Remotion's frame loop is fast once it is running.

## Formats

The same scene in several formats, counted with real tokenizers:

| Format | Tokens |
|---|---|
| Lottie, minified | 4,143 |
| Verbose JSON, minified | 2,051 |
| HTML + GSAP | 533 |
| Compact JSON with short strings, minified | 189 |
| CSS-like custom language | 183 |

Cost of the edit "make card 3 red and bring it in 10 frames later":

| Method | Tokens |
|---|---|
| Rewrite the verbose scene | 2,053 |
| JSON Patch | 29 |
| Id-based edit: `card3 fill=red delay+=10f` | 9 |

What we learned:

- **Density is what saves tokens:** defaults, presets and a stagger primitive cut tokens about 11×. The choice of syntax hardly matters.
- **Store meaning, not keyframes.** When a delay is stored as keyframes, one edit spreads into four changes.

## Vello spike

We built a small Rust program with Vello and rendered a 1080p test scene headlessly, with:

- gradients
- text
- an image
- a clip
- blend modes
- blur
- a drop shadow

| Renderer | Blur and shadow | Masks | ms per frame |
|---|---|---|---|
| `vello_cpu`, 1 thread | works | works | 6.5 |
| `vello_cpu`, 8 threads, no filters | — | — | 1.0 |
| `vello_gpu` | works | crashes ("not supported") | 6.2 overall (2.6 drawing + 0.4 copying back) |
| classic `vello` | no general blur | works | 4.5 overall (1.1 drawing) |

Further findings:

- **Copy-back is negligible:** copying a frame back from the GPU took about 0.4 ms.
- **GPU start-up is slow:** starting the GPU took about 0.85 s, which hurts short clips.
- **Filters are limited:** only blur, drop shadow, flood and offset are built in. Colour matrices, chained filters, background blur and motion blur have to be written by us.
- **The API moves fast:** a breaking change landed two days after the 0.4.0 release.

## Rasteriser comparison

A separate desk review compared nine renderers:

- **Skia:** the most complete effects and browser-grade text. It is a heavy C++ dependency (20–40 MB), and its GPU backend is mid-migration.
- **ThorVG:** strong effects and full Lottie support, but weak text shaping.
- **Rive's renderer:** no general blur.
- **Blend2D and tiny-skia:** no effects.

We chose Vello for v1 (see [D2](../design/decisions.md#d2-vello-as-the-rasteriser)). The review also found that once drawing is on native code, **encoding becomes the slowest step**, which shaped [D13](../design/decisions.md#d13-codec-licence-policy).

## Prior art

| Project | What it does | What it lacks for our goals |
|---|---|---|
| LottieCode | Compiles a compact language to Lottie | Stagger written per keyframe; no geometric checks |
| Waves | JSON scenes rendered through Remotion | Needs a browser; validates the schema only |
| MoVer (research) | Checks AI-made SVG animations with logical statements | SVG only; needs many rounds of fixing. Its statement vocabulary inspired our `assert`. |
| LottieGPT (research) | Shrinks Lottie for model training | An encoding for training, not for editing |
| Motion Canvas, Revideo, Manim | Animation as code | Code edits; browser or slow rendering |
| Theatre.js | Keeps animated values separate from code, addressed by path | Inspired our id-based edits |

## Raw data

The scripts, scene files, renders and logs from these investigations are kept outside the repository. The M0 benchmark harness will reproduce them inside `bench/`.
