Status: Draft · 2026-11-18

# The opt-in GPU path: how close, and how fast

`nakkash render --gpu` hands the float canvas's work to a GPU ([D74](../design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference)). This note holds the measurements behind D74: how far each kernel lands from the CPU function it ports, how far whole frames land, and what the GPU saves. The one thing to remember: the kernels agree with the CPU to about 1e-6 and are 14 to 16 times faster on a 4K grid, but a frame gains only as much as its time spent in those kernels, and on real templates most of the time is in effects that have no kernel yet.

All numbers are *measured* on an M4 Max through Metal, with a release build, on a machine shared with other work (load average between 30 and 200 over the day). Frame times are the `render` step of `--timings` on one thread, the fastest of three runs.

## Kernels against the CPU

`cargo test -p nakkash-gpu --test parity -- --nocapture` runs each kernel and the CPU function it ports on the same pixels: patterns with hard edges, a soft disc, a ramp, transparent areas and noise, at odd sizes. The difference is the largest of any channel of any pixel, on values from 0 to 1.

| What | Cases | Largest difference |
|---|---|---|
| Copies up and down, 8-bit bytes widened, cuts | 3 | 0 |
| Laying, each of the 17 blend modes, constant and per-pixel amounts | 34 | 5.7e-7 (color-dodge) |
| Mattes read on the GPU, four modes | 4 | 1.2e-7 |
| A filter mix by cut, opacity and region | 1 | 6.0e-8 |
| Moving pixels: shift, turn, non-uniform scale; a moving effect | 4 | 8.6e-6 (turn) |
| `gaussian-blur`, blurriness 1.5 to 900, one way or both, both edges, scaled | 6 | 1.4e-6 |
| `box-blur`, radius 2.2 to 120, 1 to 9 passes | 4 | 1.3e-6 |
| `glow`, added and alone | 3 | 9.5e-7 |
| `drop-shadow`, behind and alone | 2 | 8.9e-7 |

No case made a NaN the CPU did not. The moves differ most because the GPU works out sample positions in 32-bit floats where the CPU uses 64; at 4K coordinates that is a few ten-thousandths of a pixel.

## Whole frames

`cargo test --release -p nakkash-cli --test gpu -- --nocapture` draws the middle frame of eight examples on the CPU and with the GPU, and compares the 8-bit images.

| Example | Effects on the GPU | Largest difference | Channels that differ |
|---|---|---|---|
| `showcase/product-launch` | 3 drop-shadow, 6 gaussian-blur | 1 level | 67 of 8.3 million |
| `showcase/brand-system` | 1 gaussian-blur | 1 level | 3 |
| `advanced/ui-reveal` | 1 drop-shadow, 2 gaussian-blur, 3 glow | 1 level | 3 |
| `advanced/camera-depth` | 1 glow | 0 | 0 |
| `showcase/data-story`, `advanced/kinetic-type`, `social-916`, `title-card` | none | 0 | 0 |

A level flips where a value within 1e-6 of the CPU's sits on a rounding boundary.

## Steps on a 4K grid

`cargo test --release -p nakkash-gpu --test timing -- --ignored --nocapture`, fastest of five, each step waited for:

| Step | GPU | CPU, one thread |
|---|---|---|
| Upload 3840×2160 float pixels (127 MiB) | 12.2 ms | |
| Upload what the rasteriser drew, as bytes | 3.4 ms | |
| Download | 3.2 ms | |
| Lay by `screen` | 2.5 ms | |
| `gaussian-blur`, blurriness 50 | 9.4 ms | 156 ms |
| `glow`, radius 80 | 12.9 ms | 179 ms |

## Frames

Example frames (frame 60), and the same examples drawn at 4K: each scene wrapped as a composition at twice its size in a 3840×2160 canvas, so its blurs grow with it.

| Example | 1080p CPU | 1080p GPU | | 4K CPU | 4K GPU | |
|---|---|---|---|---|---|---|
| `advanced/ui-reveal` | 375 ms | 293 ms | 1.28× | 1681 ms | 1001 ms | 1.68× |
| `showcase/product-launch` | 424 ms | 390 ms | 1.09× | 1525 ms | 1175 ms | 1.30× |
| `showcase/brand-system` | 555 ms | 480 ms | 1.16× | 1750 ms | 1655 ms | 1.06× |
| `showcase/data-story` | 138 ms | 145 ms | 0.95× | 507 ms | 507 ms | 1.00× |
| `title-card` | 208 ms | 224 ms | 0.93× | 694 ms | 692 ms | 1.00× |
| `social-916` | 50 ms | 48 ms | 1.03× | 191 ms | 193 ms | 0.99× |
| `advanced/camera-depth` | 239 ms | 240 ms | 1.00× | | | |
| `advanced/kinetic-type` | 45 ms | 43 ms | 1.06× | | | |
| `advanced/shape-mark` | 68 ms | 67 ms | 1.02× | | | |
| `lower-third` | 20 ms | 20 ms | 1.00× | | | |

A frame with nothing for the GPU costs the same either way, within the noise: the GPU is not touched. The rest of these frames' time is in drawing paths and text, generators (`ramp`, `four-color`, `fractal-noise`, `beam`), projected layers, which stay on the CPU, and writing the PNG.

## A commercial template

Two variants of a commercial 4K template, rebranded for these tests, are kept outside the repository; their effects are what imported projects use. One frame of the first at 1080p runs 5,339 effects, among them 892 fractal noises, 872 curves, 825 glows and 675 displacement maps.

| Frame 15, 1080p | CPU | GPU | |
|---|---|---|---|
| `render` step | 434 s | 341 s | 1.27× |
| Processor time of the process | 323 s | 282 s | 1.15× |

The GPU ran 1,380 of the effects (825 glows, 468 Gaussian blurs, 86 box blurs, one move) and 1,023 other operations; 3,959 ran on the CPU, with 1,794 grids copied up (55 GiB) and 772 down (24 GiB). The two frames differ by 2 levels in 3 channels and by 1 level in 969 channels of 8.3 million.

A profile of the GPU run puts the time in projected layers (`projected::lay` and the sampling under it), lays on the CPU inside those layers' buffers, fractal noise and the other effects without a kernel: the GPU's own work is a small share of it.

## What falls back, and what to port next

This was the state when D74 first landed; [the resident frame](#the-resident-frame-measured) below is what came of it.

In that frame, by runs on the CPU: fractal-noise 892, curves 872, displacement-map 675, solid-composite 428, set-matte 338, fill 224, vector-blur 168, tint 144, find-edges 120, lens-distortion 23, polar-coordinates 16, linear-wipe 13, invert 12, shift-channels 12, radial-blur 9, iris-wipe 8, lens-blur 4, repeat-tile 1. Each costs a copy each way when the effects around it ran on the GPU.

What would gain most, in order:

1. **Projected layers on the GPU**: their resampling is the largest single cost in the profile, and it is a per-pixel homography, as the move kernel is an affine.
2. **The per-pixel colour effects** (`curves`, `tint`, `fill`, `set-matte`, `invert`, `shift-channels`, `solid-composite`): cheap kernels, and each one keeps a chain on the GPU, saving two copies.
3. **`fractal-noise` and `displacement-map`**: heavy on the CPU, and per-pixel.
4. **The radial, vector and lens blurs**, once their pull requests land.

## The resident frame, measured

After D74 first landed, the work went in this order: the frame was kept on the GPU (one submission per frame), then kernels were added group by group in the order the profile gave (effect operations, projected layers, colour and channel effects, distortions, generators, the radial, vector and lens blurs and the bevel, masks), and then inputs were kept where they were drawn.

These numbers are *measured* on an M4 Max through Metal, release builds, `render --threads 4`, fastest of two runs, on a quiet machine (load average about 5). "Before" is the `--gpu` of D74's first version; "after" is the end of this work. The commercial template's frames are its two heaviest scenes at 1080p.

| Frame | CPU | `--gpu` before | `--gpu` after | After against the CPU |
|---|---|---|---|---|
| Commercial template, scene 1, frame 30 | 11,683 ms | 11,789 ms | 1,611 ms | 7.3× |
| Commercial template, scene 2, frame 20 | 25,431 ms | 25,056 ms | 4,004 ms | 6.4× |
| `advanced/camera-depth`, frame 60 | 71.1 ms | 73.5 ms | 40.3 ms | 1.76× |
| `showcase/product-launch` | 147.3 ms | 147.8 ms | 113.6 ms | 1.30× |
| `advanced/ui-reveal` | 160.1 ms | 158.6 ms | 133.6 ms | 1.20× |
| `showcase/brand-system` | 298.9 ms | 302.1 ms | 273.8 ms | 1.09× |
| `title-card` | 72.1 ms | 73.9 ms | 69.7 ms | 1.03× |
| `social-916` | 18.6 ms | 19.3 ms | 18.0 ms | 1.00× |
| `showcase/data-story` | 54.3 ms | 55.0 ms | 61.3 ms | 0.89× |

With the GPU, the heavy scene 1 frame is the CPU's in 99.35% of its 8.3 million channels: 46,187 differ by 1 level, 5,186 by 2, and 2,282 by 3 to 18, where the loose kernels' differences (straight map readings, softened vector maps) run on through 389 effects. The eight examples differ by at most 1 level, in at most 66 channels.

The CPU path is unchanged by this work: the same frames take the same time (scene 1: 11.50 to 11.65 s after, 11.66 to 12.26 s before, over three alternating runs), and both heavy frames are the same PNG bytes before and after.

`data-story` is the one frame drawn slower: its small projected planes are laid on the GPU, which for planes this light costs more than the CPU did. `social-916` has nothing for the GPU, so it is drawn on the CPU and the GPU is not touched.

What the GPU did in the heavy frames, from the tally after the render:

| | Scene 1 before | Scene 1 after | Scene 2 before | Scene 2 after |
|---|---|---|---|---|
| Effects on the GPU | 13 | 389 | 13 | 505 |
| Effects on the CPU for want of a kernel | 378 | 2 | 494 | 2 |
| Grids copied up | 19 (0.88 GiB) | 159 (1.61 GiB) | 19 (0.88 GiB) | 195 (3.41 GiB) |
| Grids copied down | 13 (0.60 GiB) | 1 (the frame, 8 MiB) | 13 (0.60 GiB) | 1 (the frame) |
| Submissions | one per operation | 6 | one per operation | 10 |

Before, a glow combined by any operation other than adding, and every effect of the other families, ran on the CPU, so the GPU's grids kept coming back. After, the two effects left on the CPU in each frame are swirly block noise. The uploads that remain are what the rasteriser drew, as 8-bit bytes.

Four changes besides the kernels mattered:

- A buffer pool that keeps the sizes a frame uses (least recently used, 4 GiB idle at most): scene 2 went from 1,271 buffers made (88 GB) to 223 (9.4 GB), and from 14.3 to 10.8 s, before the blurs were ported.
- Exact box sums: a 96-bit integer window sum in place of a running f32 sum, at the same speed. See D74's why.
- Masks on the GPU: scene 2's 59 masks took 1.58 s of the CPU's spreading, feathering and combining.
- Inputs kept where they were drawn: in scene 1 this took the copies from 232 up and 40 down (6.8 GiB) to 179 up and 21 down (3.5 GiB), and the submissions from 41 to 22, before the blurs were ported.

### Where the time goes now

Spans timed inside one render of each heavy frame with `--gpu`, exclusive time, the frame's own thread (local profiling build, not committed):

| Span | Scene 1 (1,862 ms) | Scene 2 (4,493 ms) |
|---|---|---|
| The rasteriser drawing paths, glyphs and pictures (`vello_cpu`), 284 and 324 drawings | 907 ms (48.7%) | 1,473 ms (32.8%) |
| Waiting for the GPU at the end, and the frame's copy down | 247 ms (13.3%) | 1,724 ms (38.4%) |
| Laying layers on the GPU (recording) | 222 ms (11.9%) | 260 ms (5.8%) |
| Copying what the rasteriser drew up, as bytes | 190 ms (10.2%) | 354 ms (7.9%) |
| Clips' coverage and the mask outlines' setup | 108 ms (5.8%) | 330 ms (7.3%) |
| Swirly block noise on the CPU | 65 ms (3.5%) | 66 ms (1.5%) |
| Everything else | 6.6% | 6.3% |

The kernels themselves barely show, since the CPU only records them. Scene 2's wait at the end is the GPU still running its glows on the largest grids (5048×2844) after the CPU has finished: that frame is bound by the GPU for its last 1.7 s. Scene 1 is bound by the CPU, and half of it is the rasteriser.

### What still runs on the CPU

- Effects without a plan: `particles`, `contour-strokes`, `audio-spectrum`, `audio-waveform`.
- Values the kernels don't cover: `set-matte` read off a colour without its alpha, or off hue or saturation; swirly block noise; a `radial-blur` centred more than 2^24 px (zoom) or 2^15 px (spin) from the layer's origin.
- Motion blur of a drawing (`moved_average`), and a clip's coverage, which is exact 8-bit coverage.
- Grids, or a plane's halved copies, larger than the adapter's largest storage binding.
- Drawing paths, glyphs and pictures.

## Drawing paths on the GPU: an opt-in raster path (proposal, not built)

With everything else on the GPU, drawing is the largest share of a heavy frame: 48.7% of scene 1 and 32.8% of scene 2. A further 10.2% and 7.9% is copying those drawings up, which a GPU rasteriser would not need. So drawing on the GPU could take scene 1 from 1.6 s toward 0.6 to 0.7 s, if it cost nothing; that is an upper bound, not a measurement. Scene 2 would gain less, since its last 1.7 s are the GPU's own.

**Behind D2's interface.** D2 chose `vello_cpu` behind a small interface; today the painter calls `vello_cpu::RenderContext` directly. The first step is to carve that interface out of `render.rs`: begin a drawing of a size; set the transform, paint, fill rule and stroke; push and pop blend and clip layers; fill and stroke paths; draw glyph runs and images; finish to bytes, or to a grid in the accelerator. `vello_cpu` stays its reference implementation, bit for bit. A GPU implementation would live in `nakkash-gpu` beside the kernels, so it draws on the same device and its result is a grid the float canvas lays without a copy. Like the rest of D74 it would be opt-in and approximate, with its own parity harness against `vello_cpu`.

**Which crate.**

- **`vello` 0.12** (the GPU compute rasteriser) depends on `wgpu` ^30, `peniko` ^0.6.1 and `skrifa` ^0.49. Those are the versions we use (`wgpu` 30.0.2, `peniko` 0.6.1, `skrifa` 0.49.0), so it can take our `Device` and `Queue` and draw into a texture. A copy from that texture into a buffer, and the existing unpack kernel, would make it a grid. Its anti-aliasing is its own, so edges would differ from `vello_cpu`'s by more than a level, and whole frames could not be held to D74's one level: the limit would have to be measured, edges apart from interiors.
- **`vello_hybrid` 0.2** draws sparse strips on the CPU and fills them on the GPU. It shares `vello_common` with `vello_cpu`, so its coverage comes from the same strip generation and should land much closer to the reference. But 0.2.0 depends on `vello_common` ^0.2 and `wgpu` ^29, while we use `vello_cpu`/`vello_common` 0.4 and `wgpu` 30. It cannot share our device, nor our paint types, until it catches up. When it does, it is the better fit. Its strips are still made on the CPU, so it would take the fine rasterisation and the copies off the CPU, but not all of the drawing.

**Unknowns to measure first.** What one drawing costs on the GPU: a frame makes about 300, each now a full-frame pixmap. How much of `vello_cpu`'s 907 ms is strip generation (which `vello_hybrid` would keep on the CPU) and how much is filling. And how far `vello`'s edges land from `vello_cpu`'s on text, which most frames hold. A new dependency is scanned with `wormward` before it is added.

