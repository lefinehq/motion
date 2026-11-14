Status: Draft · 2026-11-13

# Rendering

motion turns a scene into video in five steps, and none of them involves a browser. Each frame is worked out independently, so frames can be rendered in parallel on every CPU core. Our target for the 5-second reference clip is under half a second.

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
 5. Encode    turn frames into MP4, WebM, MOV or PNG files, and mix in audio
```

Steps 1–3 never touch pixels. Checks and edits only need these three steps, which is why they are fast and cost no image tokens.

## Drawing

**Rasterising** means turning shapes and text into pixels. We use `vello_cpu`, from the open-source Vello project, and run one frame per CPU thread. We picked it after testing it by hand (see the [research note](../research/2026-11-13-baselines.md#vello-spike)):

- **Speed:** a 1080p frame with blur, shadow, masks and blend modes took about 6.5 ms on a single thread (*measured*).
- **Native features:** blur, drop shadow, clipping, masks and blend modes all work natively.
- **No start-up cost:** there is no GPU to start up, so short clips start immediately.
- **Runs anywhere:** it works on any machine, including servers without a GPU.

The rasteriser sits behind a small interface, so it can be swapped later (see [D2](decisions.md#d2-vello-as-the-rasteriser)). That interface is a **display list**: one frame as plain data, a tree of layers whose leaves are filled or stroked paths and glyph runs. `motion-raster` draws it ([D34](decisions.md#d34-a-layer-tree-renderer-on-vello_cpu)):

- A layer has a transform, an opacity, a blend mode, a clip and a list of effects. A layer that needs none of them only moves its children and costs nothing; any of them draws its children into a buffer of their own first, as a nested composition always is.
- Effects are a registry: a name, typed parameters and a function that draws onto the layer's isolated content. The first is `fill`, which recolours the content and keeps its alpha by painting the colour "source-atop".
- **Blending space.** `vello_cpu` blends gamma-encoded sRGB values only. That is what projects that do not linearise expect, so imported masters match; a frame that asks for linear-light blending ([D21](decisions.md#d21-linear-light-float-compositing)) is refused with an error until the rasteriser can blend in linear light (it would need to convert paints and layer buffers to linear before compositing and back when packing pixels).
- **From scene to display list.** The engine maps each evaluated frame (`crates/motion-engine/src/render/`): a visible node becomes a layer with its canvas transform, opacity and effects; shape paints become filled or stroked outlines under their group transforms, their opacity folded into the colour; text runs become glyph runs in their paint order, and under animators each glyph takes its own transform, opacity, colours and stroke width, with a glyph that fades while it draws both a fill and a stroke drawn into a buffer of its own so the stroke does not show through. Faux bold is a stroke in the fill colour, its width 1/24 of the size at 9 px falling to 1/32 from 36 px (the curve Skia's synthetic bold uses); a stroke on the text widens by the same amount. Faux italic slants each glyph 12° about its origin. Neither changes advances, so layout stays as the font sets it. A nested composition is a layer clipped to its canvas over its background, holding its own nodes at the time the scene evaluated.
- **Speed through the whole pipeline** (*measured*, M4 Max, release build): the ten-layer story scene in `crates/motion-engine/tests/fixtures/story.json` evaluates in 0.010 ms and maps to a display list in 0.007 ms more, then draws in 17.3 ms with float compositing on one thread, 4.8 ms with the integer pipeline and 27.6 ms with portable scalar code (medians over 40 frames); `render_frames` on all 14 threads averages 1.7 ms per frame over 100 frames (`cargo test --release -p motion-engine --test timing -- --ignored --nocapture`).
- **Speed** (*measured*, M4 Max, one thread, release build): a 1080x1920 frame with ten story-template layers (a plate, two rotated squares, a clipped group, three text lines, a stroked arrow, two recoloured groups) takes 16.7 ms with float compositing, 4.5 ms with the faster integer pipeline, and 26.7 ms with portable scalar code (`cargo test --release -p motion-raster --test timing -- --ignored --nocapture`).

**Compositing** combines the layers: effects, masks, mattes, blend modes and 2.5D depth. In v1 it runs on the CPU, written once in Rust. That gives us one implementation, bit-identical results and the same behaviour on every server. Layers with no effects skip this step entirely.

**GPU acceleration** is planned for phase 2. It covers the heavy cases: 4K, strong motion blur, large blurs and procedural noise. If the CPU version misses our speed target at milestone M2, it moves into v1 (see [D3](decisions.md#d3-cpu-first-compositor)).

**Motion blur** blends several in-between moments of the same frame. Because motion can compute any moment exactly, this costs no extra work in the engine. It only multiplies the drawing work.

## Encoding

In a fast renderer, encoding is usually the slowest step.

| Platform | H.264 / HEVC | Always available |
|---|---|---|
| macOS | VideoToolbox (built into the OS) | AV1, VP9, ProRes 4444 with alpha, PNG sequence |
| Windows | Media Foundation (built into the OS) | same |
| Linux with an NVIDIA or Intel/AMD GPU | NVENC / VAAPI | same |
| Linux, CPU only | OpenH264, downloaded from Cisco on first use | same |

**Licence policy.** The motion binary contains no GPL code and no patent-encumbered software encoder. H.264 and HEVC come from the operating system or the GPU vendor, whose licences cover them, or from Cisco's OpenH264 binary under Cisco's own terms. This is how web browsers handle it too. See [D13](decisions.md#d13-codec-licence-policy).

## Speed

| | Reference clip (5 s, 1080p, 30 fps) |
|---|---|
| Remotion, warm | 3.3 s (*measured*) |
| HyperFrames, warm | 5.3 s (*measured*) |
| **motion** | **≤ 0.5 s end to end, cold start ≤ 150 ms (*target*)** |

Where the time goes in browser-based tools:

- **Fixed start-up cost:** launching the browser and bundling the code.
- **Capturing each frame** from the browser.

motion has neither.

**Long videos** can be split into chunks, rendered by several processes or machines, and joined. This works because no frame depends on the one before it.

## Repeatability

- **Frames:** on the CPU path, the same scene, motion version, operating system and CPU type always produce identical pixels. Golden images are drawn with portable scalar code instead of the processor's SIMD instructions, so the same PNGs compare exactly on every machine CI runs on; delivered frames use the fastest instructions, which may differ from the goldens by a rounding step.
- **Encoded files:** these are not guaranteed identical, because hardware encoders are not.
- **The future GPU path:** it will promise frames that look the same, not frames that are bit-identical.

## Install and distribution

- **One binary per platform:** macOS (Apple silicon and Intel), Linux (x64 and arm64) and Windows (x64).
- **npm:** `npx <package> render scene.json` works without installing a browser or a compiler.
- **Bindings and crates:** Node bindings with TypeScript types, and Rust crates on crates.io.
- **WebAssembly:** the parts that do not draw pixels (read, compile, evaluate, check, edit) stay WebAssembly-compatible, so a browser-based editor or preview can reuse them later.

The package name is still open (see [Q1](decisions.md#q1-name-and-packages-deferred)).
