# Track mattes

**Goal:** a layer can show only where another layer of its composition draws, or only where it does not, by that layer's alpha or its luminance. s737 reveals its title through three mattes, and s740 raises its hashtag through an inverted one.

**A correction.** The ledger's first notes said that s737's matte layers drew as plain white over the frame. That was wrong. The white was a full-frame solid that the manifest already ignores. Projects switch off the layers they use as mattes, and the import already kept them hidden. Without mattes, s737's title was simply missing.

**Done means:**
- The format has a `matte` key, and validation, compile, display and raster carry it.
- `motion import` maps the project's track mattes and no longer reports them as unsupported.
- The ledger shows s737 and s740 better and nothing worse.
- Goldens cover the four modes, a moving matte, a matte inside a nested composition, and a matte chain.

## The format

```json
{"id":"title","text":"LIFE","matte":"reveal alpha"},
{"id":"reveal","rect":"600x200","fill":"#FFFFFF","hidden":true}
```

- **What `matte` holds:** `"<layer id> <mode>"`, where the mode is `alpha` (the default), `alpha-inverted`, `luma` or `luma-inverted`.
  - The layer shows where the matte layer is opaque, or bright.
  - With an inverted mode, it shows where the matte layer is transparent, or dark.
- **Where the matte layer comes from:** another layer of the same layer list. It is drawn as it would be at that time: its transform, parents, opacity, effects and span. The result is used as a stencil in the same space.
  - Outside its span, it draws nothing. So an `alpha` matte then hides the layer, and an `alpha-inverted` one shows all of it.
- **Whether the matte layer is drawn itself:** that is up to its own `hidden`. Projects switch off the layers they use as mattes, and the import keeps that switch. A hidden layer still serves as a matte.
- **Validation:** a matte names another layer of the same list, and following mattes never comes back to a layer. A matte layer may have its own matte; its masked result is what masks.

## Work, in order, one pull request

1. **Format.** `matte` in the document, the layer and validation (the layer exists, no cycle, the mode is known), `LAYER_KEYS`, `scene-format.md`. A compatibility scene uses all four modes.
2. **Scene graph.** `Node.matte: Option<(usize, MatteMode)>`, resolved at compile like `parent`.
3. **Display list.** A layer with a matte becomes an isolated `Layer` with `matte: Some(Matte { source: Box<Node>, mode })`. The source is the matte layer's own display node, built even when that layer is hidden.
4. **Raster.**
   - The source is drawn with the same transform into a canvas-sized buffer of its own.
   - That buffer becomes the rasteriser's mask. All four modes are computed in one place (alpha or Rec. 709 luma of the premultiplied pixels, inverted or not) and passed as bytes through `Mask::from_parts`.
   - The layer is pushed with that mask.
   - Goldens: the four modes, a matte with opacity and a transform, a matte in a nested composition.
5. **Import.** Each project matte maps to `matte` with its mode. Files that store the matte layer's id use it; older files use the layer above. The matte layer keeps its own visibility. A kind the file codes but the scene has no mode for is reported. Report counts and the import snapshots change.
6. **Measure.** Ledger before and after, research note, board.

## What could go wrong

- **Cost.** The matte source is drawn twice: once into the mask, and once in the frame if it is not hidden. Masks are canvas-sized. That is acceptable at the sizes we render; the timing test will show it.
- **Luminance.** Luminance in sRGB or in linear light changes luma mattes slightly. We read the encoded values, as the rasteriser's own luminance mask does. Neither sample uses a luma matte yet, so this is unmeasured.

## Result

- **s740:** better on every number over all frames (failing where drawn 4.93% → 4.76%), and settled numbers unchanged.
- **s737, mixed:**
  - The failing fraction fell (89.2% over all frames, from 97.0%).
  - The mean error rose (0.50 → 0.56). Its title now shows, in the right shape and place, but white where the reference's is black: the colour comes from a colour control through an expression.
- **Both:** mattes are no longer reported as unsupported. What remains is expressions.
