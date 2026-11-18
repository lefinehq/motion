Status: Draft · 2026-11-13

# Rendering

nakkash turns a scene into video in five steps, and none of them involves a browser. Each frame is worked out independently, so frames can be rendered in parallel on every CPU core. Our target for the 5-second reference clip is under half a second.

## The pipeline

```
scene.json
   │
   ▼
 1. Read      parse the JSON, expand presets and short strings, validate
   │
   ▼
 2. Compile   expand repeats, nested comps and layout; shape text  ──► checks and edits work from here
   │
   ▼
 3. Evaluate  for frame t: every layer's exact position, size, colour and opacity
   │
   ▼
 4. Draw      rasterise each layer (Vello), then composite: effects, masks, mattes, blend modes, depth
   │
   ▼
 5. Encode    turn frames into MP4, MOV or PNG files, and the mix into WAV
              (WebM, and sound inside MP4 and MOV, later)
```

Steps 1–3 never touch pixels. Checks and edits only need these three steps, which is why they are fast and cost no image tokens.

## Drawing

**Rasterising** means turning shapes and text into pixels. We use `vello_cpu`, from the open-source Vello project, and run one frame per CPU thread. We picked it after testing it by hand (see the [research note](../research/2026-11-13-baselines.md#vello-spike)):

- **Speed:** a 1080p frame with blur, shadow, masks and blend modes took about 6.5 ms on a single thread (*measured*).
- **Native features:** blur, drop shadow, clipping, masks and blend modes all work natively.
- **No start-up cost:** there is no GPU to start up, so short clips start immediately.
- **Runs anywhere:** it works on any machine, including servers without a GPU.

The rasteriser sits behind a small interface, so it can be swapped later (see [D2](decisions.md#d2-vello-as-the-rasteriser)). That interface is a **display list**: one frame as plain data, a tree of layers whose leaves are filled or stroked paths and glyph runs. `nakkash-raster` draws it ([D34](decisions.md#d34-a-layer-tree-renderer-on-vello_cpu)):

- A layer has a transform, an opacity, a blend mode, a clip and a list of effects. A layer that needs none of them only moves its children and costs nothing; any of them draws its children into a buffer of their own first, as a nested composition always is.
- Effects are a registry: a name, typed parameters and a function that draws onto the layer's isolated content. The first is `fill`, which recolours the content and keeps its alpha by painting the colour "source-atop".
- **Blending space.** `vello_cpu` blends gamma-encoded sRGB values only. That is what projects that do not linearise expect, so imported masters match; a frame that asks for linear-light blending ([D21](decisions.md#d21-linear-light-float-compositing)) is refused with an error until the rasteriser can blend in linear light (it would need to convert paints and layer buffers to linear before compositing and back when packing pixels).
- **Float pixels stay floats.** Pixels the rasteriser's own code makes (effects that read pixels, masks, filter layers, motion blur of a drawing made once, planes) are premultiplied floats. With `--quality precise`, the default, they are laid on a float canvas the frame's size and rounded to 8 bits once, when the frame is packed: a filter layer, a composition's effects, a matte or a plane reads what the layers below made at full precision. What the vello context draws (paths, glyphs, pictures and layers of them) goes over a clear target and is laid on the canvas, source-over, before the next float layer; source-over is associative, so that equals drawing it on the canvas. A layer that blends otherwise with what the canvas holds is drawn alone and laid with the blend mode's formula in floats ([`BLEND_MODES`](../../crates/nakkash-raster/src/blend.rs)). A frame with no such pixels is drawn by the context alone, as before. `--quality fast` lays them into the context as 8-bit pictures, so a filter over a dim gradient bands there. *Measured* on a dim radial ramp graded four stops, 940 px of one row: 111 colours graded on its own layer, and through a filter layer or a composition 30 before (the 8-bit levels the grade spread apart, flat runs up to 45 px), 111 after (runs up to 14 px).
- **From scene to display list.** The engine maps each evaluated frame (`crates/nakkash-engine/src/render/`): a visible node becomes a layer with its canvas transform, opacity and effects; shape paints become filled or stroked outlines under their group transforms, their opacity folded into the colour; text runs become glyph runs in their paint order, and under animators each glyph takes its own transform, opacity, colours and stroke width, with a glyph that fades while it draws both a fill and a stroke drawn into a buffer of its own so the stroke does not show through. Faux bold is a stroke in the fill colour, its width 1/24 of the size at 9 px falling to 1/32 from 36 px (the curve Skia's synthetic bold uses); a stroke on the text widens by the same amount. Faux italic slants each glyph 12° about its origin. Neither changes advances, so layout stays as the font sets it. A nested composition is a layer clipped to its canvas over its background, holding its own nodes at the time the scene evaluated.
- **Speed through the whole pipeline** (*measured*, M4 Max, release build): the ten-layer story scene in `crates/nakkash-engine/tests/fixtures/story.json` evaluates in 0.010 ms and maps to a display list in 0.007 ms more, then draws in 17.3 ms with float compositing on one thread, 4.8 ms with the integer pipeline and 27.6 ms with portable scalar code (medians over 40 frames); `render_frames` on all 14 threads averages 1.7 ms per frame over 100 frames (`cargo test --release -p nakkash-engine --test timing -- --ignored --nocapture`).
- **Speed** (*measured*, M4 Max, one thread, release build): a 1080x1920 frame with ten story-template layers (a plate, two rotated squares, a clipped group, three text lines, a stroked arrow, two recoloured groups) takes 16.7 ms with float compositing, 4.5 ms with the faster integer pipeline, and 26.7 ms with portable scalar code (`cargo test --release -p nakkash-raster --test timing -- --ignored --nocapture`).

**Compositing** combines the layers: effects, masks, mattes, blend modes and 2.5D depth. In v1 it runs on the CPU, written once in Rust. That gives us one implementation, bit-identical results and the same behaviour on every server. Layers with no effects skip this step entirely.

**GPU acceleration** is opt-in: `nakkash render --gpu` hands the float canvas's work to a GPU through `wgpu` (Metal on macOS, Vulkan or DX12 elsewhere), for faster previews of heavy frames ([D74](decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference)). Paths, glyphs and pictures are still drawn on the CPU; the GPU lays layers by every blend mode with their clips, masks and mattes, mixes filter layers, moves pixels, and runs the effects with a plan (`gaussian-blur`, `box-blur`, `glow`, `drop-shadow`). Any other effect runs on the CPU, its pixels copied back and forth at its boundary, and the command lists it after the render. The frames approximate the CPU's: each kernel is held to a measured tolerance against the CPU function it ports, and a whole frame to one 8-bit level. A machine without a hardware GPU says so and draws on the CPU; a software adapter is never used. Every check, `diff`, fidelity measurement and delivered file stays on the CPU.

**Motion blur** blends several in-between moments of the same frame. Because nakkash can compute any moment exactly, this costs no extra work in the engine. It only multiplies the drawing work. A layer whose content and effects stay the same over the shutter (a picture, a `comp` layer, most effect stacks) is drawn once and moved to each moment; any other, particles included, is drawn at each moment ([D57](decisions.md#d57-motion-blur-samples-a-layers-motion-over-the-shutter)).

**Projected layers** are the rasteriser's half of 2.5D ([D58](decisions.md#d58-25d-layers-are-planes-projected-through-a-camera), [plan](../plans/2026-11-21-depth.md)). A layer with `Layer::projection` is a plane seen through a camera: a 3×3 projective matrix (`Homography`) takes its own coordinates to its parent's, in place of its transform (`crates/nakkash-raster/src/projected.rs`).

- **Drawing.** Its children, masks, clip and effects are drawn flat in its own coordinates, into a buffer of its own, as an isolated layer's are. Effects get the buffer's space, so blur sizes and points stay in layer units.
- **The buffer's region** is the part of the plane the canvas sees: the frame's inverse image in front of the camera, worked out as five half-planes clipping the box the layer draws in. That box is its bounds or its children's (glyphs measured from their outlines), grown by each effect's reach (`Reach`, a row of the effect registry) and cut to its clip. The region is then widened by how far its masks and effects read. A plane that draws everywhere, such as one with particles, is cut towards its horizon where 64 × 64 of its units fall within one px.
- **The buffer's scale** is the most the matrix magnifies that region, at most 4×, and lowered so neither side passes 8,192 px.
- **Laying.** Each canvas px is mapped back onto the plane. A px whose ray meets the plane behind the camera stays clear, so a plane crossing the camera is cut there with no clipping of outlines. The others are sampled from a chain of halved copies, made only when a footprint needs one. The level comes from the footprint the px has on the plane (the Jacobian of the map back), trilinearly, with up to 16 probes along the footprint's long axis, so planes seen nearly edge-on neither alias nor blur across. The buffer's clear border keeps edges antialiased.
- **Afterwards** opacity, blend mode and matte (in the parent's coordinates) apply as for a flat layer. A projected filter layer's region is drawn on its plane; its effects run on the canvas, measured by the projection's affine match at the middle of what is seen.
- **Motion blur** ([D57](decisions.md#d57-motion-blur-samples-a-layers-motion-over-the-shutter)) is `Projection::samples`: the drawing is made once and laid through the map at each moment, averaged at 1/n each. These are whole maps, not moves as `Layer::motion` holds, since the map at the frame may be singular (a plane seen edge-on). A layer with both is refused.

*Measured* (unit tests in `crates/nakkash-raster/tests/unit/projected.rs`):

| Check | Result |
|---|---|
| Identity and whole-px moves against the flat path | the same bytes |
| Turned 0.5 rad and scaled 1.3× against the flat path | mean 0.39 levels, at most 60 on edges, from resampling |
| Card turned 60° about Y at a zoom of 2,666.7 px | 200.86 px across, 200.85 by hand |
| Plane crossing the camera | its front part only, cut within a px of where it should be |
| Checkerboard tipped 84° back, against 16 × 16 supersampling | mean error 0.032 of white; 0.059 with one probe per px; 0.49 point-sampled |
| Masked, blurred card turned 50°, against the flat drawing at 4× laid by hand with 4 × 4 samples | mean alpha error 0.0005 |
| Motion-blur samples against `Layer::motion` with the same moves | the same bytes |

*Measured* (M4 Max, one thread, release build, median of 21): a 1920 × 1080 card turned 35° about Y through the default camera, its buffer 2,563 × 1,462 px, draws in 53.3 ms with float compositing and 48.7 ms with the integer pipeline; drawn flat, 3.6 ms and 1.4 ms (`cargo test --release -p nakkash-raster --test timing -- --ignored --nocapture`). Most of it is sampling, two bilinear reads per px where the plane shrinks: with one read the frame takes 23 ms, with none 11 ms.

## Encoding

In a fast renderer, encoding is usually the slowest step.

`nakkash render --out <file>` encodes when the name ends in `.mp4` or `.mov`; a folder gets PNG frames. Frames are drawn on every core and handed to the encoder in order (`Session::render_in_order`), at most two per thread waiting, so memory does not grow with the video's length. It grows with the threads instead: each holds the frame it draws, and a frame with 3D planes seen up close holds about a gigabyte. `--threads N` draws on at most N threads, for a machine shared with other work. The frames and the rows of each frame share those threads ([D75](decisions.md#d75-a-frame-is-drawn-on-every-thread-of-its-render)): each thread takes the next frame while frames are left, and the threads without one work the rows of the frames still drawn, so one frame alone, such as `--frame N`, is drawn on all of them. `--timings` shows `encode` apart from `render`.

| Codec | Where | From | Container | Default |
|---|---|---|---|---|
| H.264 | macOS | VideoToolbox, built into the OS | `.mp4`, `.mov` | on macOS |
| HEVC | macOS | VideoToolbox | `.mp4`, `.mov` | |
| ProRes 4444 with alpha | macOS | VideoToolbox | `.mov` | |
| AV1 | everywhere | `rav1e`, compiled in | `.mp4` | elsewhere |
| H.264 | Windows; Linux with a GPU; Linux on CPU | Media Foundation; NVENC or VA-API; Cisco's OpenH264, fetched on request | | *planned* |
| VP9, AV1 in WebM | everywhere | | `.webm` | *planned* |

**How it is built** ([D46](decisions.md#d46-video-is-encoded-in-a-crate-of-its-own-behind-one-codec-trait), [plan](../plans/2026-11-16-encode.md)):

- `nakkash-encode` knows nothing about scenes. One colour module turns straight-alpha RGBA8 into what codecs take: BT.709 limited-range 4:2:0 with alpha drawn over black, or, for codecs that keep alpha, 16-bit 4:4:4 with straight alpha.
- Each codec is one `VideoEncoder`; each container one `Muxer`. MP4 and QuickTime are our own writer on `mp4-atom`'s boxes: packets stream to a scratch file and the file is laid out with its index first.
- The system's encoders are reached through `nakkash-videotoolbox`, the one crate allowed unsafe code ([D48](decisions.md#d48-unsafe-code-lives-only-in-platform-bindings)).
- `--video-quality draft|web|master` is a row in one table per codec; ProRes has `master` only.

**Speed and size** (*measured*, M4 Max, release build, 60 frames of `examples/launch.json` at 1080p; PSNR of the decoded RGB against the drawn frames, which 4:2:0 chroma caps near 53 dB):

| | draft | web | master | time (web) |
|---|---|---|---|---|
| H.264 | 18 KB, 37.7 dB | 62 KB, 52.4 dB | 101 KB, 52.9 dB | 0.3 s |
| HEVC | 27 KB, 40.7 dB | 64 KB, 52.5 dB | 90 KB, 52.9 dB | 0.3 s |
| AV1 | 17 KB, 46.2 dB | 24 KB, 51.5 dB | 42 KB, 52.7 dB | 5.7 s |

**Dark gradients.** `web` asks VideoToolbox for quality 0.8, up from 0.65 on 2026-11-15. At 0.65 a night-sky gradient of the demo film (luma 4 to 27 over 700 px) came out in blocks with purple and blue blotches: the 99th percentile of the error, after a 9 px blur, was 3.0 levels, and 1.26 at 0.8 (1.22 at 0.85, 1.0 at 0.95). It costs about half again in size (launch: 40 to 62 KB; the sky: 225 to 406 kbit/s). Measured, so not tried again:
- dither before the encoder (a deband-and-dither filter) does not survive the system's H.264 and HEVC encoders, even at a constant 6 Mbit/s;
- a bit-rate target instead of a quality helps less than 0.8 does, since the system's encoders spend what still frames need, not the target;
- a lower AV1 quantizer leaves the gradient error where it is.

The times are the whole render, drawing included, best of three. The colour conversion reads each pixel once and converts bands of rows on threads of their own: 3.3 ms for a 1080p frame, from 11.1 ms when it read each pixel twice on the one thread that hands frames to the encoder, which held encoding near 90 fps. A scene heavy with effects stays bound by drawing: the 40 s demo film takes 27 s either way.

ProRes 4444 writes the same 60 frames in 1.4 s, about 4 MB, and decodes to the drawn frames with alpha within one level.

**Licence policy.** The nakkash binary contains no GPL code and no patent-encumbered software encoder. H.264 and HEVC come from the operating system or the GPU vendor, whose licences cover them, or from Cisco's OpenH264 binary under Cisco's own terms. This is how web browsers handle it too. See [D13](decisions.md#d13-codec-licence-policy).

## Sound

A scene's sound is mixed apart from its frames ([D56](decisions.md#d56-audio-layers-mixed-at-48-khz-and-read-by-effects), [plan](../plans/2026-11-20-audio.md)). `nakkash render scene.json --out mix.wav` writes it alone, and a video of a scene that plays sound holds the mix of the frames drawn as its second track.

- **Reading.** `nakkash-audio` reads WAV itself and MP3, FLAC and Ogg Vorbis through pure Rust decoders, chosen by the file's first bytes. Each file is decoded once per session, whole, folded down to stereo by where its speakers stand (ITU-R BS.775), and resampled to 48 kHz by our own band-limited kernel, which gives the same samples on every machine.
- **Mixing.** Each audio layer is a route from the canvas down through `comp` layers. The mix asks each route what plays once a millisecond (the file position, the gain from every `volume` on the way, the speed from every `stretch`) and moves linearly between, so fades have no steps; around a layer's start or end it asks every sample. A layer that plays at its own rate copies samples straight from the file; one stretched reads through the kernel, narrowed so nothing folds back. Layers are summed without a limiter, and a render reports a peak above full scale.
- **In the video.** The mix follows the frames in the file, in one chunk, so the index ahead of both still lets a player start at once.
  - **MP4** takes AAC-LC at 192 kbit/s from the system's encoder ([D13](decisions.md#d13-codec-licence-policy)): AudioToolbox on macOS. The encoder's priming (2,112 frames) is cut by an edit list, so the sound starts with the first frame. Where the system has no AAC encoder nakkash can use (Linux and Windows today), a scene with sound is refused in MP4, with `.mov` or `.wav` offered instead.
  - **MOV** takes 16-bit PCM on every platform, from the mix's floats with triangular dither of one step, from a fixed seed, so the same mix gives the same bytes.
  - On the 40 s demo film both decode to the mix's 1,920,000 frames with no offset: AAC within 54 dB SNR, PCM within 73 dB.
- **Reading sound while drawing.** `audio-spectrum` and `audio-waveform` ask the mix for the window they need at each frame, so frames still draw alone, in any order, on every core. A track written `@level` follows a sound: a scene with one compiles twice, first to find the sound's routes, then with the levels measured once per frame of its list as keys (step `listen`).

*Measured* on an M4 Max (release build), for a 60 s scene at 30 fps with two layers of a 1:44 MP3, one stretched 1.5×, a spectrum effect, and a scale following the bass:

| Step | Time |
|---|---|
| Decoding and resampling the MP3, once | 0.42 s |
| `listen`: the bass level at 1801 frames | 0.05 s |
| `mix`: 60 s of both layers | 0.31 s |

## Speed

| | Reference clip (5 s, 1080p, 30 fps) |
|---|---|
| Remotion, warm | 3.3 s (*measured*) |
| HyperFrames, warm | 5.3 s (*measured*) |
| **nakkash** | **≤ 0.5 s end to end, cold start ≤ 150 ms (*target*)** |

Where the time goes in browser-based tools:

- **Fixed start-up cost:** launching the browser and bundling the code.
- **Capturing each frame** from the browser.

nakkash has neither.

**Long videos** can be split into chunks, rendered by several processes or machines, and joined. This works because no frame depends on the one before it.

## Repeatability

- **Frames:** on the CPU path, the same scene, nakkash version, operating system and CPU type always produce identical pixels. Golden images are drawn with portable scalar code instead of the processor's SIMD instructions, so the same PNGs compare exactly on every machine CI runs on; delivered frames use the fastest instructions, which may differ from the goldens by a rounding step.
- **Encoded files:** these are not guaranteed identical, because hardware encoders are not.
- **The GPU path** (`--gpu`) promises frames within the tolerances of [D74](decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference), not bit-identical ones: drivers compile its kernels differently.

## Install and distribution

- **One binary per platform:** macOS (Apple silicon and Intel), Linux (x64 and arm64) and Windows (x64).
- **npm:** `npx <package> render scene.json` works without installing a browser or a compiler.
- **Bindings and crates:** Node bindings with TypeScript types, and Rust crates on crates.io.
- **WebAssembly:** the parts that do not draw pixels (read, compile, evaluate, check, edit) stay WebAssembly-compatible, so a browser-based editor or preview can reuse them later.

The package name is still open (see [Q1](decisions.md#q1-name-and-packages-deferred)).
