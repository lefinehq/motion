Status: Draft · 2026-11-13

# Features

motion aims to match professional motion-design software for the work most videos need: layered composition, precise animation, effects and depth. Features come in two tiers. The core tier ships in v1; the extended tier follows. Some areas of video work are deliberately left out.

Words in *italics* are defined in the [glossary](../glossary.md).

## Core tier (v1)

**Composition**
- Layers, *nested comps*, *pivots* and parenting
- Layout in rows, columns and grids
- *Filter layers*

**Animation**
- Behaviours, property tracks and keyframes with bezier easing (see [Scene format](scene-format.md#animation-at-three-levels-of-detail))
- *Modifiers*: jitter, loop, link and follow
- Per-character, per-word and per-line text motion

**Shapes and paths**
- Rectangles, ellipses, paths and SVG
- Path operations:
  - *draw-on*, where a stroke draws itself
  - clone arrays
  - inset and outset
  - corner rounding
  - boolean combine

**Compositing**
- Masks with feather, expansion and modes
- *Mattes*: alpha, luma and their inverses
- All standard blend modes
- Motion blur, controlled by *shutter angle*

**Depth (2.5D)**
- Layers that rotate and move in depth (`rotateX`, `rotateY`, `z`)
- A camera with perspective, dolly and orbit
- Optional depth of field

**Effects**
- **Blur:** Gaussian and directional
- **Light and depth:** glow, drop shadow, inner shadow
- **Outline and colour:** stroke, fill, tint
- **Colour correction:** levels, curves, hue and saturation
- **Generators:** gradient, procedural noise, vignette

**Audio**
- Place, trim, volume and fades
- Mixed into the output file

**Checks**
- Analytic checks and pixel checks (see [Edits and checks](edits-and-checks.md))

**Import and variants**
- An exporter inside the desktop tool, with a coverage report
- The variant surface: roles, beats, style profile, locks
- Swap, reframe, cut down, localise and reinterpret (see [Variants](variants.md))

## Extended tier (phase 2)

- More modifiers
- Particles
- Retiming and frame interpolation
- Distortion: noise displacement, warp, bulge
- Colour lookup tables (LUTs)
- Video layers
- True 3D: glTF models, lights, materials and shadows
- Lottie import, so existing animations can be brought in
- HDR and a 32-bit-per-channel pipeline
- GPU acceleration for heavy scenes

## 3D, in two steps

The scene structure supports 3D from the start, so step 2 needs no rewrite.

1. **v1: 2.5D.** Flat layers move and turn in 3D space, seen through a camera: card flips, perspective tilts, parallax and camera moves. This covers most 3D in motion graphics.
2. **Phase 2: true 3D.** Models, lights and materials, rendered and blended with 2D layers.

## Left out on purpose

See [Scope](scope.md#out-of-scope).
