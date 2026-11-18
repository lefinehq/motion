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

In that frame, by runs on the CPU: fractal-noise 892, curves 872, displacement-map 675, solid-composite 428, set-matte 338, fill 224, vector-blur 168, tint 144, find-edges 120, lens-distortion 23, polar-coordinates 16, linear-wipe 13, invert 12, shift-channels 12, radial-blur 9, iris-wipe 8, lens-blur 4, repeat-tile 1. Each costs a copy each way when the effects around it ran on the GPU.

What would gain most, in order:

1. **Projected layers on the GPU**: their resampling is the largest single cost in the profile, and it is a per-pixel homography, as the move kernel is an affine.
2. **The per-pixel colour effects** (`curves`, `tint`, `fill`, `set-matte`, `invert`, `shift-channels`, `solid-composite`): cheap kernels, and each one keeps a chain on the GPU, saving two copies.
3. **`fractal-noise` and `displacement-map`**: heavy on the CPU, and per-pixel.
4. **The radial, vector and lens blurs**, once their pull requests land.
