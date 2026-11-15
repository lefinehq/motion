Status: Done · 2026-11-15

# Motion blur

**Goal:** layers that move are blurred along their motion, as [D57](../design/decisions.md#d57-motion-blur-samples-a-layers-motion-over-the-shutter) says, so the import can carry a project's motion blur instead of reporting it:

```json
{"motion":1, "size":"1080p", "shutter":"180 -90 16",
 "layers":[{"id":"ball", "shapes":[…], "mblur":true, "anim":{"x":"0 -> 1920 @0-12f"}}]}
```

## What the corpus says

- **Reach.** In the latest run, motion blur is reported in 39 measured samples, 406 times: the sixth blocker. A further 12 samples use the Motion Blur effect, which is not this.
- **Settings.** The projects keep the angle, phase, samples per frame and adaptive limit in each composition's `cdta`. The switch that turns it on for a composition is bit 3 of its flags.

## The format

- **`mblur`** on a layer: `true` blurs it along its motion. The default is `false`.
- **`shutter`** on the scene and on a composition: `"angle phase samples"`.
  - The angle runs from 0 to 720.
  - The phase runs from -360 to 360.
  - The samples per frame run from 2 to 64.
  - The default is `"180 -90 16"`, and `0` turns blur off.
  - A composition without one uses the default, not the scene's: the projects keep the setting per composition.
- **Validation.** `mblur` on a layer that draws nothing is a warning from `check`, not an error.

## Drawing

- **Sample times.** For a layer with `mblur` in a composition whose shutter is open, the engine evaluates the frame at `n` moments:
  - they start at `phase / 360` of a frame from the frame's time;
  - they run evenly over `angle / 360` of a frame.
- **Sample count.** `n` is the samples per frame. Where the layer's corners move more than one px per sample between the first and last moment, `n` grows to match, up to 8 × the samples per frame.
- **Drawing the samples.**
  - Each moment's state draws the layer as at a frame, with its content, effects and parents.
  - The rasteriser draws each sample into one isolated group at `1/n` opacity, adding (`plus`).
  - The group then takes the layer's blend, opacity and matte.
- **Nested compositions.** A layer inside a nested composition is sampled at its composition's own times. A `comp` layer with `mblur` blurs its whole drawing as it moves.

## The import

- **Settings.** Each composition's shutter comes from its `cdta`: the angle at 0xAC, the phase at 0xB4, the samples at 0xC8. The adaptive limit at 0xC4 is kept as the upper bound when it is lower than 8× the samples.
- **Switches.** A layer's motion blur switch becomes `mblur` when its composition's motion blur switch is on.
- **Reported.** A switch whose composition's motion blur is off is not reported, since it changes nothing. A 3D layer's blur is drawn flat, as the layer is.

## Steps

Each step is one pull request, with its tests and docs.

1. **Decision.** D57 and this plan.
2. **Format and drawing.** `shutter` and `mblur`, sample times, adaptive sample counts, and the averaged group in the rasteriser. Golden tests for a moving rect, a spinning shape, a 360° shutter and a nested composition.
3. **The import.** The `cdta` fields, the switches, and a measurement on the corpus.

## Out of scope

- Frame blending.
- Mask motion blur on its own.
- Pixel motion blur from the Pixel Motion Blur effect.
