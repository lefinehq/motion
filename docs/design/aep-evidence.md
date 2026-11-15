Status: Living · 2026-11-14

# What we know about `.aep` by inference

Most of what the decoder reads is either documented or follows from the file's own structure. This page lists what it reads by inference instead: what we believe a field means, and the check that holds that belief over every local sample ([hardening plan, H4](../plans/2026-11-14-hardening.md#h4-evidence-for-inferred-format-knowledge)). When a sample breaks a belief, the belief is wrong. We fix the belief, then the code.

The checks run in the local sweep:

```text
MOTION_AEP_SAMPLES=~/aep-samples cargo nextest run -p motion-import --run-ignored only
```

## The composed-line cache

A text document stores the lines it was last laid out in (`/2` of the document's engine data). The text geometry check holds our layout against them, so these beliefs carry its numbers.

| Belief | Check | Held, 2026-11-14 |
|---|---|---|
| B1. A line's origin (`/L /0`) sits on its baseline (`/L /10`) | equal within 0.001 px | 113 lines of 47 layouts |
| B2. A segment's `/15 /0` counts UTF-16 code units, and every paragraph's end counts as one: a document's lines together count its text plus one | equal | 47 of 47 |
| B3. In point text, a segment's origin (`/S /0`) is where its line starts: `0`, `-w/2` or `-w` for left, centred and right lines. Here `w` is the segment's glyph run width (`/G /1`) less the tracking after the last character | within 0.01 px | 83 lines |

Other fields we read by inference, without a check yet:
- the glyph ids of a glyph run (`/G /5`), which the check does not use;
- a segment's cumulative advances (`/15 /7`). No local sample stores them, so the check compares them only where present.

### What the beliefs taught us

- **B3, first version, was wrong.** It took the centre as `-w/2`. A sample with tracking 200 at 30 px starts 3 px right of that: alignment leaves out the tracking after the last character, while the stored width includes it. Our layout aligns tracked text including that tracking, so a tracked, centred line sits half a tracking step left of where it should. This is open; the sample that shows it needs a font we do not have locally yet.
- **Faux bold is not tracking.** The faux bold title of the first sample is centred on its full stored width, the extra space after its last character included. So faux bold's extra advance (`FAUX_BOLD_TRACKING`) is aligned like an advance, not like tracking.

## Cameras

The import reads a camera's rig and fills in what the project leaves out ([depth plan](../plans/2026-11-21-depth.md), step 5). These beliefs were measured once, by hand, over the 241 local projects and the corpus's reference frames; the sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-15 |
|---|---|---|
| C1. Bit 6 of `ldta` flags B (0x40) is a camera's two-node switch: it turns towards its point of interest, its anchor point. | The format notes measured a new two-node camera writing 0x44 and one with auto-orient off writing 0x04. | All 95 cameras set it, so no local sample shows a one-node camera. All 95 store their point of interest. |
| C2. A camera that leaves out `ADBE Camera Zoom` has a zoom of 2,666.6666666 px in a composition 1,920 px wide: a 50 mm lens on 36 mm film, `width · 50/36`. | A new camera sits at minus its zoom from its point of interest. | Of the 70 cameras that leave the zoom out, 57 store a still position and point of interest: 52 sit exactly 2,666.6666666 px in front of it, and 5 that much plus a whole number of px, moved by hand. Of the 18 that store a zoom and a still position, 13 sit exactly their zoom in front, and 5 were moved. |
| C3. A camera that leaves out `ADBE Position` sits at the composition's centre, at minus the default zoom (C2). The default does not follow the camera's own zoom. | m811 leaves out its position and zoom; once its turn about X has settled, its 3D layer at depth 0 matches the reference as a flat layer would, with a drawn error of 0.015. m800 leaves out its position and keys its zoom from 2,666.67 to 25,000. At 1.5 s the reference shows its text 1.5 times as wide as the flat drawing, which does not grow: the camera stays where it is while it zooms. | 11 cameras leave their position out, 7 of them in compositions 3,840 px wide. |

What is left open:
- **Other widths.** C2 is measured at 1,920 px only. The seven cameras in compositions 3,840 px wide leave out both their zoom and their position. With both at their defaults, the plane at depth 0 shows at 100 % whatever the zoom, so their frames show the zoom only through perspective, which is not measured yet. The import takes `width · 50/36` for every width, as the scene's default camera does.
- **The point of interest.** Every camera stores it, so its default is unmeasured. The import takes it as an anchor point's default: the origin, as for a layer without a source.
