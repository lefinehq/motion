Status: Living · 2026-11-16

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

## Stars and polygons

A star group (`ADBE Vector Shape - Star`) becomes a `star` item ([scene format](scene-format.md#shapes-masks-effects-and-compositions)). Its geometry was measured once, by hand, against the corpus's reference frames: one property of every star in an imported sample is changed by a factor, each variant is drawn, and each is compared with the reference only at the pixels the factor moves. The mean error there is lowest where the belief holds. The sweep does not check these beliefs yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| S1. The corners stand at equal angles around the position, the first straight up from it at rotation 0, and run on clockwise. `Rotation` turns them clockwise, in degrees. | m713 draws 6-corner polygons without rotation. Turned by 0°, 10° and 30° more, they err by 0.034, 0.340 and 0.594 (frame 2). m619 turns a 4-corner polygon by 138.43°: as stored it errs by 0.024, turned the other way by 0.246 (frame 8). | 26 polygons of m713, 9 of m619 |
| S2. A rounded corner's handles run along the circle through it, both ways, each `roundness · 2π r / (4 n)` long for `n` points of radius `r`. | m619: `n = 4`, `r = 588.35`, roundness 67 %. Factors 0.5, 0.75, 0.9, 1, 1.1, 1.25, 1.5 and 2 on the handles err by 0.177, 0.108, 0.064, **0.013**, 0.066, 0.106, 0.168 and 0.295 (frames 8 and 12). Finer, at frame 8: 0.96, 0.98, 1, 1.02 and 1.04 err by 0.177, 0.094, **0.059**, 0.128 and 0.215. m620: `r = 560.35`, roundness 44 %. Factors 0.5, 0.9, 0.96, 1, 1.04, 1.1 and 2 err by 0.227, 0.049, 0.022, **0.007**, 0.024, 0.052 and 0.457 (frames 6, 10 and 14). | 9 polygons of m619, 7 of m620, within 2 % |
| S3. `ADBE Vector Star Type` is 1 for a star and 2 for a polygon, and a star group that leaves it out is a star of 5 points, outer radius 100 and inner radius 50. | Of the 172 star groups in the 206 local projects, the 96 that store a type store 2 and are drawn as polygons. The 76 that store none, among them those of m573, m625, m647 and m769, all have 3 points and an inner radius half the outer: as stars, each inner corner lies on a side and they draw triangles, which the references show. m647 stores its radii. Scaling its inner radius by 0.8, 0.9, 1, 1.1 and 1.2 errs by 0.220, 0.182, **0.146**, 0.152 and 0.167 (frames 4 and 8). m625 stores 3 points and nothing else, and its 62 triangles need the defaults 100 and 50 to be triangles. | 172 groups |

Other things we take without evidence:
- **A star's roundness.** No local project rounds the corners of a type-1 star: the 96 groups that store an inner roundness store 0, and the 38 that store an outer one other than 0 are polygons. A star takes S2 for its points and for its inner corners, each on its own circle, with `n` its number of points, as a polygon does. A rule of `2π r / (2 n)`, a quarter of the arc between two neighbouring corners of the polygon counted twice, would make the handles twice as long; on the polygons above, twice errs by 0.295 against 0.013.
- **Direction.** No star group stores `ADBE Vector Shape Direction`. Among the other shapes, 96 store 2 and 6 store 3; none of them is drawn in a sample. The import maps 1 to `clockwise` and 3 to `counterclockwise`, which runs the same corners from the same first one the other way round, and reports 2, whose meaning is not known.
- **Fractional points.** Every stored point count is a whole number and none is animated in the corpus; the `Benchmark` and `Star` fixtures animate it (5 to 6, 6 to 8) but have no reference frames. The scene counts the whole part of `points`, so a star animating from 5 to 6 points gains its sixth at the last key.
- **Ranges.** The project stores 0 to 100 as the bounds of the point count, the radii and the roundnesses, yet m619 stores an outer radius of 588: these are the bounds of the slider, not of the value. The scene takes 0 to 100 points (m562 stores 100), radii from 0 and roundness of any sign and size.
- **m625, m778 and m801.** Their stars are the only thing the import reported, but their references show other compositions: m778 and m801 show the title of the root composition `Neon Title 03`, not the three hexagons of the root composition `Frames` the manifest picks, and m625 shows a promotional reel, not `Pano 2`. Drawing the stars therefore makes them err more, not less, until the manifest picks the composition the preview shows.

## Anchor points

A layer's anchor point is where its own space is pinned to its position, and its children's positions are measured in that space. These beliefs were measured once, by hand, over the 198 corpus projects and their reference frames; the sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| A1. A layer with a source (a composition, footage, a solid or a null) stores its anchor point in fractions of its source's size, as it stores its masks; a layer without one (shape, text, camera) stores it in px. | The fixture `HamburgerArrow.aep` stores `[0.5, 0.5]` for a null over a 120 by 120 solid, and an export of the same project made through the host's scripting API reads `[60, 60]`. m650 stores `[0.5, 0.5]` on two nested nulls over 100 by 100 solids; read as px, its logo sits 98.5 and 99 px right and down of the reference and its text 49 and 50 px; read as fractions, the failing content falls from 3.16 % to 0.96 %. m640 stores `[6.12, 3.45]` on a null: as fractions, its frame moves onto the reference's (36.54 % to 17.20 % failing). Templates that drive such a null's anchor by an expression write `[50, 50]` over a stored `[0.5, 0.5]`: the same point. | 694 stored anchors on layers with a source, in 64 projects: none beyond 8.25 in x or y. 2,547 on layers without one: 2,434 beyond 10. |
| A2. A null that leaves its anchor out rests it at its top-left corner; another layer with a source, at its middle. | The null half: see the change of 2026-11-15 in the changelog (10 samples closer). The middle: 7,005 layers over compositions leave it out, and the corpus's passing samples draw them centred. | |

What is left open:
- **Depth.** A 3D layer with a source stores its anchor's depth too. Only one template family stores one that is not 0 (`Null 33`, `[-3.33, 8.25, -1.32]`, in m571 to m575, m779 and m783), so its unit is unmeasured; the import keeps the depth in px.


## Distortions

Distortion effects keep no formula in the project, only their parameters. These beliefs say how each moves the layer's pixels. They were measured once, by hand, against the corpus's reference frames; the sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| D1. `ADBE Mirror` reflects across the line through its `Reflection Center` (`-0001`, a point in fractions of the layer's effect space) turned by its `Reflection Angle` (`-0002`, degrees clockwise). At 0° the line is upright, the left side stays and the right side shows it reflected; what falls outside the layer's box does not show. | m611 draws two spectra from 0.32 to 0.50 of its 1,080 px width, left of a centre stored as `[0.5, 0.5]`. Its reference frames 10 and 16 show their white from x = 338 to 741, centred on 539.5. Folding that ink about x = 540 leaves 0.42 and 0.38 of it unmatched, about 535 or 545 between 0.72 and 0.87. Had the right side stayed, which these layers leave empty, nothing would show. | 15 mirrors in m611, m612, m621 and m812. All store the centre `[0.5, 0.5]` and none stores an angle. |

What is left open:
- **The angle.** No local sample turns the line, so the angle is taken clockwise, as the project's other angles turn, without a measurement. At 90° the bottom shows the top.
- **Where the mirrors are drawn.** The four samples draw their mirrors on audio spectra. m612 and m812 draw theirs along a mask (`ADBE AudSpect-0004`, which stores 0 on layers that do have a mask), which the scene does not draw yet, and m611's reference shows a background and a title placement its imported frame does not, so mirroring moves their error little.
