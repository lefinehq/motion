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

## Text selectors

A range selector (`ADBE Text Selector`) becomes a `range` selector ([scene format](scene-format.md#text-animators)). Its ease was measured once, by hand, against the corpus's reference frames: the imported scene is drawn as imported, with `ease-high` and `ease-low` swapped and with neither, at the reference's own times, and each is compared with the reference only at the pixels where the three differ. The sweep does not check this belief yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| T1. A selector's choice from its shape, 0 to 1, passes through the cubic from (0, 0) to (1, 1) whose first handle is (low, 0), or (0, -low) when low is negative, and whose second is (1 - high, 1), or (1, 1 + high), before the amount; high is `ADBE Text Levels Max Ease`, low `ADBE Text Levels Min Ease`, in percent. | m788 eases low by 100 only (a square `Start` and a ramp-up `Offset`). Mean error at frames 8, 11 and 12: as stored 0.185, 0.045, 0.057; swapped 0.500, 0.520, 0.504; without ease 0.358, 0.412, 0.100. m773 eases high by 100 only (a ramp-up `Start`). At frames 12 and 13: as stored 0.017, 0.008; swapped 0.314, 0.307; without ease 0.185, 0.168. | 2 samples, 5 frames, 116,000 px |

What is left open:
- **Random order.** `ADBE Text Randomize Order` is still reported, and the order is not known. The open renderers do not record it: one shuffles with a generator seeded by a fixed number, ignoring `ADBE Text Random Seed`, and another draws from an unseeded generator, so neither repeats from one run to the next of the original. m569 shows two texts with seed 17, `GLITCH UP` and `GLITCH DOWN`, each part moved by its turn in a ramp, so one frame of its preview gives every part's turn. Taking turn 0 first, the parts in order are `I`, space, `U`, `L`, `T`, `C`, `G`, `P`, `H` for 9 parts and `T`, `D`, `O`, `L`, `C`, `H`, `G`, `W`, space, `N`, `I` for 11. The two agree as if the selector drew one number per turn from a sequence the seed fixes, and gave turn `k` to the part whose number ranks `k`-th: the first 9 turns of the 11 rank their parts as the 9 turns do. Neither the fixed-seed shuffle nor the common generators (the C libraries', the minimal standard, the Mersenne twister, xorshift, the 48-bit generator, published LCGs) seeded with 17 give those ranks. A seed of 0 is documented as taken from the animator group, which most corpus selectors use.

## Expression selectors

An expression selector (`ADBE Text Expressible Selector`) becomes a `parts` selector ([D64](decisions.md#d64-a-selector-may-choose-each-part-by-its-own-amount)): its amount's expression is evaluated once per part. These beliefs were measured once, by hand: two scenes are drawn at the reference's own times and compared with the reference only at the pixels where they differ. The sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| E1. `textIndex` counts the parts from 1, and `textTotal` is how many there are; the expression's amount, in percent, chooses its part. | Every part given the amount of the one before it, as an index from 0 would give: m554 errs 0.666 against 0.315 as baked (32,000 px), m805 0.390 against 0.094 (159,000 px). Against the selector left out, as before: m805 0.067 against 0.427 (424,000 px). | 2 samples |

What is left open:
- **One number.** An expression that gives one number, such as `if (textIndex == n) {100} else {0}`, chooses its part by it along every axis, as an open renderer reads it. The samples measured give two or three numbers, so this is taken without evidence.
- **Axes.** The amount holds three numbers. The scene takes the first for every value of one number (opacity, rotation, colours, tracking) and each axis its own for offsets and scales, as an open renderer does. On m618, whose keyed amounts run from `[100, 100, 100]` to `[-100, 0, 100]` under a position and an opacity, giving every value the second number errs less at the pixels it moves (0.205 against 0.224, base 0.208), and the first number everywhere much more (0.351). One sample does not settle it.
- **After other selectors.** In m619 and m809 the expression selector follows a range selector, whose choice it replaces and reads as `selectorValue`. These stay reported.

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
| D2. `ADBE Polar Coordinates` with `Type of Conversion` 1 (the definition lists `Rect to Polar`, `Polar to Rect`, default 2) shows, at the point at angle φ counterclockwise from straight up of the layer's middle and `r` px from it, the layer φ/2π of the way across its box and √2·`r` px below its top. Each pixel goes `Interpolation` (`-0001`, a share from 0 to 1) of the way from itself towards that point. | m799 draws a spectrum along the middle row of a 1,920 by 1,080 solid and converts it at 0.806. Its reference ring (frame 12) is a circle of radius 381 px, between 377 and 389 px in every 10° away from its loud bands, about a point 49 px above the layer's middle, with the loud bands (x 520 to 910 in our drawing of the spectrum) as long spokes from 100° to 170° counterclockwise from up. We drew our spectrum through the belief and its variants and compared the ink with the reference's, outside the logo (overlap of both, each grown by 3 px, over either): √2·`r` overlaps 0.30, 1.3·`r` 0.15 and 1.5·`r` 0.23; columns running clockwise 0.09. The ring the belief draws has radius 382 px. | 1 effect, in m799. |
| D3. `CC RepeTile` grows the layer's box by `Expand Right`, `Left`, `Down` and `Up` (`-0001` to `-0004`, px of the layer) and fills what it adds with copies of the box, aligned with it. `Tiling` 1 (`Repeat`, the definition's default) copies it as it is. On a collapsed composition the box is that composition's canvas where it lands, not the canvas it draws into. | m583 scrolls the 443 by 44 composition `Text01_LT03` (`MIXKIT` bold, then thin) through a window, collapsed, expanding it 743 px right and 741 left. Reference frames 5 and 8 show the words repeating at the composition's width, through the layer's scale. With the composition's canvas as the box, our tiles show the same words in the same order and phase. With the canvas it draws into as the box, which the engine gave collapsed layers before, the tile is 1,920 px wide and the strip ends after one copy. | 1 collapsed layer (m583). The 12 other corpus samples that put a distortion on a collapsed layer (m565, m576 to m580, m591, m597, m598, m600, m601, m785) draw the same with the new box, to the fourth decimal, except m565 settled (0.1364 → 0.1365). |
| D4. `Tiling` 17 (`Brick`) shifts every other row of tiles by half a tile. The row of the box itself is not shifted. | m713 tiles the 230 by 120 composition `Line_Particle`, a dash, as bricks. In reference frame 4, the dashes away from the hexagon (pixels darker than 248) overlap ours by 0.938 (both, over either) with `brick`, and by 0.442 with `repeat` or `unfold`. | 7 brick tilings, in m549, m613, m713, m715 and m716. |
| D5. `Tiling` 11 (`None`) grows the box with nothing in it. | m587 to m590 name such an effect "Extend text box" and set it just before a beam that draws in the grown box. The beam is not drawn yet, so no frame shows the grown box. | 9 in m587 to m590. |

What is left open:
- **The angle.** No local sample turns the line, so the angle is taken clockwise, as the project's other angles turn, without a measurement. At 90° the bottom shows the top.
- **Where the mirrors are drawn.** The four samples draw their mirrors on audio spectra. m612 and m812 draw theirs along a mask ([E1](#effects-that-follow-a-mask)), and m611's reference shows a background and a title placement its imported frame does not, so mirroring moves their error little.
- **Polar interpolation.** The overlap peaks a little past the stored share: 0.297 at 0.81, 0.314 at 0.83 and 0.307 at 0.85. The ring's centre sits 49 px above the layer's middle in the reference and 64 px with the share as stored, which 0.85 would match. The scene takes the share as stored.
- **Polar coordinates on other shapes of layer.** One 16:9 layer cannot tell whether the radius scale follows the height, the width or neither: √2·`r` with the columns across the width is what it shows, and the scene takes that for every box.
- **Polar to rect** is taken as the inverse, unmeasured: no local sample uses it.
- **m799's dashes.** Its reference breaks the ring into short spokes; ours into short arcs. The spokes come from the venetian blinds drawn before the conversion, which the reference cuts into columns where the scene cuts rows at a direction of 0° (720° in m799). Turning m799's blinds by 90° in its scene takes it from 4.67% to 4.46% failing over all frames.
- **RepeTile unfold** (`Tiling` 4) is taken as every other column flipped across and every other row upside down, as `motion-tile` mirrors. Its two samples do not show it: m595 tiles on adjustment layers a few px wide, and m819 inside a matte.
- **The other 12 RepeTile tilings**, from `Checker Flip H` to `Slide`, are not set in any local project and are reported. So is `Blend Borders` (`-0006`) other than 0, which none sets.
- **m713's dashes** needed its `Fill` effect drawn opaque for D4: the project's fill colour stores alpha 0 (from a colour control) and the reference draws the dashes grey. The import now writes colours opaque ([C1](#colours)), and they show.
- **m583's text colour.** The reference draws the words white where the project's colour control stores black, so its error does not fall with the tiles.

## What expressions read

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| X1. An expression reads a 2D layer's anchor point, position and scale as two components, though the project stores three; a 3D layer's, a camera's and a light's as three. | A gaming lower-third family (m571 to m575, m739, m783) sizes its logo with a rig on a 2D null: its parent's scale expression gives `[thisComp.width, thisComp.height]`, two components, and the child divides each component of its own scale by its parent's, `for (i = 0; i < s.length; i++)`. With three components the third is `NaN` and the original would show an error; with two the logo's world scale is 2 by 2, and the logo lies on the reference's: m783 1.72 % → 0.93 % failing, m571 7.40 % → 1.85 %, m573 9.17 % → 1.78 %. | Over 42 corpus samples under 25 % failing, 5 draw closer and none farther. |

## Effects that follow a mask

A path parameter (`pard` control 12), such as Audio Spectrum's Path (`ADBE AudSpect-0004`), names one of its layer's masks. Its `cdat` holds 0 whatever it names; the mask is in a `tdli` beside it. These beliefs say how it is named and how the spectrum lays its bands along it. The first is checked by hand over every local sample, the layout measured against m812's reference frames; the sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| E1. The `tdli` beside a path parameter holds the id of a mask of the same layer: the big-endian `u32` at byte 8 of that mask's `mkif`. 0 names none. The id is neither the mask's index nor the number in its name. | m812's spectra store `tdli` 6 on layers whose one mask, `Mask 5`, has `mkif` id 6, and 1 on layers whose `Mask 1` has id 1. m594's stroke stores 5 for its only mask, `Mask 1`, id 5; the Masks fixture's `Mask 1` and `Mask 2` have ids 2 and 4. m621's two spectra store 0 on layers with a mask, and its reference draws them as straight bars along their points. | 135 path parameters in 18 projects carry a `tdli`: 44 `ADBE AudSpect-0004`, 79 `ADBE Fill-0001`, 9 `ADBE Stroke-0001`, 3 `APC Vegas-0050`. The 27 that are not 0 each match the id of exactly one mask on their layer. Over all 1,305 masks, no layer has two with one id. |
| E2. Audio Spectrum draws its bands along the mask its Path names, from the path's first vertex, evenly by length: band `k` of `n` at `k / (n − 1)` of an open path's length, both ends included, and at `k / n` round a closed one. Its points and its polar path go unused. Side A is to the left going along the path, as along the line from its start to its end. | m812, reference frame 5 (2.0 s), about the ring's centre at (540, 960), with our digital bands drawn as bars for the comparison. Round `Mask 1`, a circle of radius 220 drawn counterclockwise from its top, 100 bands: folded onto their 3.6° spacing, ours and the reference's ink line up with no shift, where bands centred in their spans (`(k + ½) / n`) would be 1.8° off. Their bars stand on the circle and reach inward, as side A says: the reference's ink runs from 195 to 220 px out, ours from 170 to 220. Along `Mask 5`, an open quarter circle of radius 239, 32 bands: the reference's bars are 2.90° apart, as `k / (n − 1)` gives (`k / n` gives 2.81°), and 14 of ours fall within 0.2° of one of its bars. | 21 spectra along a mask, in m549, m612 and m812. m549's and m612's projects ship their sound compositions empty (and m612 its background), so no frame of ours draws their bands. |

What is left open:
- **Heights.** m812's ring bars reach about twice as far as the reference's; the scale from loudness to px is the spectrum's own, not the path's.
- **Digital bands.** m812's bands without a stored display, `Digital` by the definition, are solid bars in its reference at a thickness of 6, where the scene draws columns of squares; m621's 40 px digital bands are squares. How the original spaces its squares is not measured yet.
- **Waveform.** Audio Waveform has the same Path parameter; no local sample uses the effect, so the scene's waveform follows a mask the same way without a measurement.
- **Animated paths.** No sample keys a path parameter; one that does is reported.

## Colours

A colour property stores four channels, its alpha among them. This belief says what the original draws of that alpha. It was checked by hand over every local sample; the sweep does not check it yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| C1. The original draws every colour it stores opaque: the alpha a colour carries, stored or given by an expression, changes nothing drawn. Where a colour can be see-through, an opacity of its own says so (a fill's opacity, a shape fill's). | m713, m714 and m716 colour their shapes and `ADBE Fill` effects by expressions that read colour controls storing alpha 0, so the fills, strokes and dashes they drive come out at alpha 0: 72 in m713, 44 in m714, 17 in m716. Their references show every one of them opaque. Written opaque, m713 goes from 67.84 % to 5.60 % failing over all frames, m714 from 87.70 % to 3.48 % and m716 from 29.72 % to 5.36 %; the 24 other samples whose scenes change score the same. | 10,982 stored colours under 122 match names in the 198 samples; the 145 with alpha below 1 are all effect parameters and colour controls: `ADBE Fill-0002` 48 (each driven by an expression), `ADBE Tint-0001` and `-0002` 77, `ADBE Laser-0009` 7, `ADBE PhotoFilterPS-0002` 1, `ADBE Color Control-0001` 12. No reference shows a colour see-through by its alpha. |

What is left open:
- **Text.** A text run's fill and stroke come from its document, not a colour property; a run with neither is written clear, as before.
- **Effects the scene does not draw yet.** The beams of `ADBE Laser` and Photo Filter store alphas below 1; they will draw opaque when mapped.

