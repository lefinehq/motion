Status: Living · 2026-11-17

# What we know about `.aep` by inference

Most of what the decoder reads is either documented or follows from the file's own structure. This page lists what it reads by inference instead: what we believe a field means, and the check that holds that belief over every local sample ([hardening plan, H4](../plans/2026-11-14-hardening.md#h4-evidence-for-inferred-format-knowledge)). When a sample breaks a belief, the belief is wrong. We fix the belief, then the code.

The checks run in the local sweep:

```text
NAKKASH_AEP_SAMPLES=~/aep-samples cargo nextest run -p nakkash-import --run-ignored only
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
- **Random order.** `ADBE Text Randomize Order` is still reported, and the order is not known. The open renderers do not record it: one shuffles with a generator seeded by a fixed number, ignoring `ADBE Text Random Seed`, and another draws from an unseeded generator, so neither repeats from one run to the next of the original. Four texts in three samples give the order the original draws, each read from its preview, frame by frame, as the turn at which each part of the text moves or appears:

  | Sample | Seed | Parts | How the turns show | Parts by turn, turn 0 first |
  |---|---|---|---|---|
  | m569 | 17 | 9, `GLITCH UP` | a ramp moves each part by its turn | 2, 6, 7, 1, 3, 4, 0, 8, 5 |
  | m569 | 17 | 11, `GLITCH DOWN` | as above | 3, 7, 8, 1, 4, 5, 0, 9, 6, 10, 2 |
  | m638 | 20 | 9, a site name with no space | `Start` rises from 0 to 100 and each part appears at its turn | 4, 3, 1, 7, 5, 0, 6, 2, 8 |
  | m838 | 20 | 5, `BEACH` | `End` falls from 100 to 0, so the last turn appears first | 2, 1, 0, 4, 3 |
  | m594 | 30 | 16, `Natalie & Anthon` | `Start` rises from 0 to 100; the two spaces cannot be seen | 8, 13, 3, 0, 2, 5, 12, 6, 4, 15, 10, 1, 11, 14, the spaces 7 and 9 somewhere among them |

  Parts count from 0 and include spaces. Both texts of each seed agree as if the selector drew one number per turn, in turn order, from a sequence the seed alone fixes, and gave the part with index `i` the turn whose number ranks `i`-th: the first 9 turns of seed 17's 11 rank their parts as its 9 turns do, and the first 5 turns of seed 20's 9 in m638 rank theirs as m838's 5 turns do, in another project, layer and font. A shuffle of the parts themselves would not keep that agreement, and the chance of it is 1 in 9! and 1 in 5! for the two pairs.

  Nothing tried gives these orders. Tested on all five texts, and counted only when two texts or more match: about fifty generators (the C libraries' `rand` and `random` on three platforms, the minimal standard with both multipliers and with a shuffle table, the Mersenne twister in 32 and 64 bits seeded as integers and as arrays, the 48-bit generator, xorshift in 32, 64 and 128 bits, xoroshiro, xoshiro, splitmix, a permuted congruential generator, a combined Tausworthe, KISS, the subtractive generator, Wichmann-Hill and twenty published LCGs, taken whole and by their high bits); the seed as stored, offset by -3 to 12345, multiplied, xored with common constants or combined with the number of parts; 0 to 11 numbers dropped first, and every number or every other one used; the parts ranked by the numbers either way, a forward or backward swap shuffle, and an insertion of each turn at a place the number picks, which keeps the agreement above. Numbers drawn apart for each turn, by hashing the seed and the turn (eight integer and sine hashes, and the first numbers of each generator above, over twenty ways of combining the two), do not give them either. The selector therefore keeps the order of the parts and reports `random order`. 118 of the corpus's 149 selectors in random order store a seed of 0, which is documented as taken from the animator group.

## Text options of the whole text

A text layer's `ADBE Text More Options` holds what applies to the whole text: whose anchor each character takes (`ADBE Text Anchor Point Option`), how far the anchors move (`ADBE Text Anchor Point Align`), how fills and strokes stack across characters (`ADBE Text Render Order`) and how characters blend with each other (`ADBE Text Character Blend Mode`). They become the text-level `style` keys `anchor-by`, `anchor-align` and `paint-across` ([scene format](scene-format.md#text-animators)). These beliefs were checked once, by hand, over every local sample: 29 projects hold the group, on 118 text layers.

| Belief | Evidence | Held, 2026-11-17 |
|---|---|---|
| M1. Anchor grouping 1 is each character's own anchor, the default; 2, 3 and 4 are the word's, the line's and the whole text's. | Values found: 2 on 30 layers of m618, m619, m735, m809 and the excluded m637; 3 on 5 layers of m566; 4 on 4 layers of m716. None of the scored layers with 2, 3 or 4 scales, turns or skews its characters, so no reference frame shows the anchor; drawn with each value, those samples draw the same. | Taken without evidence beyond the order of the choices |
| M2. Grouping alignment is two percentages that move every anchor from the middle of its baseline: the first of the part's width, the second of the size of its font. | 7 scored samples move it: m565 `[0, -29]`, m735 and m839 `[0, -30]`, m802, m815 and m816 `[0, -31]`, m791 `[-25, 0]`. The vertical values sit near half the cap height of their fonts, as a centre for turning and flipping capitals would. Drawn with the second percentage taken of the font's size, of the line's ascent, of each glyph's height above its baseline and of each glyph's ink box, m565, m791, m802, m815, m816 and m839 differ by at most 0.0007 in content error and 0.01 points of failing share: their scaled characters enter under a random order or blur that the scene does not draw yet. m839, whose characters flip about the moved anchor, shows the flip only after the reference's last frame. | The unit of the vertical percentage is open |
| M3. Render order 1 keeps each character's own paint order, 2 draws every fill over every stroke, 3 every stroke over every fill. | Values found: 1 on 41 layers (m567, m568, m569, m570, m617, m748, m768, m826, m847, and the excluded m614, m615, m631), always with blend mode 1; 2 on m595 and the excluded m591; 3 on m735 and the excluded m637 and m638. m768 passes every reference frame over its floor with 1. Of the scored layers with 2 or 3, only m595's `Gaze` title is stroked (26 px over 200 px type, where the order would show), in a composition its reference does not show. | 1: 1 sample. 2 and 3: taken without evidence |
| M4. Blend mode 1 blends nothing: each character is drawn over the ones before it. | Every layer that stores the blend mode stores 1, 41 layers in 12 projects, m768 among them. | 41 of 41 layers; other values are reported |

Before, the import reported the group as `text anchor grouping and render order` wherever it held a property: 85 times in 23 scored samples. Now none is reported, and no other value is. Over all frames of those 23 samples, the drawing changes only in m815 (99.85 % → 99.84 % failing, content error 0.2623 → 0.2630) and m816 (62.96 % → 62.95 %); every other sample, and every other text sample measured (60 in all), draws the same to the fourth decimal.

## Scaled characters

A character style's horizontal and vertical scale (`horizontal_scale` and `vertical_scale` in the document's style) become the `horizontal-scale` and `vertical-scale` style keys ([scene format](scene-format.md#text)). The first belief was measured once against the stored layouts (the [composed-line cache](#the-composed-line-cache)) of 13 samples: m562, m571, m619, m620, m634, m639, m640, m641, m647, m651, m738, m769 and m809, imported with their manifest's fonts. The sweep does not check it yet.

| Belief | Evidence | Held, 2026-11-17 |
|---|---|---|
| S1. A horizontal scale multiplies every advance and the tracking after it: a line is set as if at its size times the scale. | Set unscaled, the stored width over ours is exactly the scale for lines of one style, with or without tracking: m641's `NEW` (666 px, tracking 50, scale 0.93) is 1748.49 px stored against 1880.12 ours (0.9300), m640's `THE` 360.69 against 387.84, m620's `PRICE: $ 700.000.000` 657.83 against 707.36. Tracking kept unscaled would leave m641's `NEW` 1752.4 px wide. Set at the scaled size, ours is 1748.51. Over the 13 samples' texts, lines within the check's 0.25 px of their stored start, width and baseline go from 26 to 56 of 129, and whole texts from 20 to 30 of 83. | Every line of one scaled style whose font we have, in 9 samples |
| S2. A scale leaves the distance from one baseline to the next as it is: automatic leading stays a multiple of the unscaled size. | m640's two-line texts (182 px, scale 0.93) keep their stored baselines 0 and 143, and m641's four lines (52 px) keep 0, 46, 92 and 138. | 6 multi-line texts |

Other things we take without evidence:
- **Vertical scale.** Two scored samples hold one, m562 (1.04) and m651 (0.98), on texts of one line. The glyphs are drawn taller from their baseline, and the line's ascent grows with them. No reference shows it apart from other changes.
- **What else the line's other lines leave.** Lines that end in a space store a width that includes it, as for unscaled text (m619's `YOUR IMAGE `), and box text's first baseline is off by about 141 px in m620, scaled or not. Both are open, and neither comes from the scale.

Before, the import reported `scaled characters` 47 times in 12 scored samples and set them unscaled. Over all frames:

| Sample | Failing | Content error | Frames passing over the floor |
|---|---|---|---|
| m640 | 17.20 % → 10.08 % | 0.0892 → 0.0140 | 9 → 9 of 20 |
| m641 | 44.96 % → 29.36 % | 0.2737 → 0.1186 | 2 → 2 of 14 |
| m634 | 44.65 % → 40.80 % | 0.3452 → 0.2955 | 6 → 7 of 20 |
| m620 | 33.66 % → 32.97 % | 0.1892 → 0.1827 | 6 → 6 of 20 |
| m639 | 56.31 % → 64.50 % | 0.4172 → 0.4509 | 2 → 2 of 16 |
| m619, m647, m651, m769 | within 0.07 points | within 0.0005 | unchanged |
| m562, m738, m809 | unchanged | unchanged | unchanged |

m639's six stacked `CREATION` titles are skewed in the reference, which the scene does not draw yet. Set at their right width, their unskewed letters cover less of the reference's slanted ones than the wider letters did. Its stored layouts come closer (worst line 100.55 → 51.33 px).

## Ranges in parts

A range selector stores its range twice, in percent (`ADBE Text Percent Start`, `End`, `Offset`) and in parts (`ADBE Text Index Start`, `End`, `Offset`), and `ADBE Text Range Units` says which it uses. The `range` selector's `units` setting keeps the choice ([scene format](scene-format.md#text-animators)). The belief was measured once against the corpus's reference frames; the sweep does not check it yet.

| Belief | Evidence | Held, 2026-11-17 |
|---|---|---|
| R1. Range units 2 take start, end and offset from the index properties, as numbers of parts; 1 takes them from the percent properties. Each selector reads only the triple its units name. | 10 selectors in 6 samples (m566, m795, m802, m815, m816, m839) use units 2. Each stores both triples, its percent triple at the full range (0, 100, 0), so read as percent they chose every part all the time. Read as parts: m566's `BOXED` and `titles` (start 50, end 0, offset 25, subtracting) now leave their letters in place, and its failing share over all frames goes from 81.66 % to 75.58 % (content error 0.2498 → 0.1643). m795, m802, m815 and m816, whose ranges sweep their letters from an offset of 29 down to -17 parts, each err less: 82.95 % → 82.80 %, 86.01 % → 85.93 %, 99.85 % → 99.84 % and 62.96 % → 62.76 %. Their letters also enter in a random order, which the scene does not draw yet. | 6 of 6 samples err less; m839's selector moves only after its reference ends |

What is left open:
- **Line breaks.** A part counts characters other than line breaks, as `char` counts for percent ranges. m802's `Wedding` and `Slideshow` (16 characters on two lines) end their range at 17, past the last character either way. No sample tells whether the original counts the break.
- **A triple left out.** Every local selector in parts stores all three index properties, so what a missing one defaults to is not known. The import then leaves the scene's own default.

## Tracking type

A text animator's `ADBE Text Track Type` says on which side of each character its tracking adds space. The scene adds it after each part ([scene format](scene-format.md#text-animators)) and has no other side. These beliefs were checked once, by hand, over every local sample.

| Belief | Evidence | Held, 2026-11-17 |
|---|---|---|
| K1. The tracking type changes nothing while the animator's tracking amount is 0 all the time or left out. | 359 animators store a tracking type. Every one whose type is not 1 has a tracking amount of 0: m639, m640 and m641 (type 2) among the scored samples. The 200 animators whose tracking is ever not 0 all hold type 1, some of them as a single key (m574, m575, m779). | 359 of 359 |

What is left open:
- **Type 1.** Whether type 1, the one the corpus tracks with, adds its space after each part (as the scene draws it), splits it before and after, or adds it on both sides, is not settled. The readings differ by under 0.2 points of failing share over all frames of m566, m567, m570, m574, m575 and m748, and no reading is ahead on all six. At the pixels where they differ in five reference frames of m566, adding the space on both sides errs least (0.4127 against 0.4210 after, at 0.5 s). The text there is off by about 0.4 for other reasons, so this does not settle it either.

Before, the import reported `tracking before and after` 17 times in 6 scored samples: wherever the type was not 1 (all with no tracking), and wherever the type was keyed, even with one key at 1. Now it reports a type only where the animator tracks; no local sample does. A property keyed with one value throughout counts as that value for every row that reads a setting or a default.

## Blurred characters

An animator's blur (`ADBE Text Blur`) becomes the animator's `blur` ([scene format](scene-format.md#text-animators)), across and down apart. The corpus holds 16 in 9 scored samples, none animated and none under an expression: 114 or 113 both ways in m648, m760, m802, m815 and m816, 19 or 20 in m760 and m795, 5 in m826, 90 down only in m793, 0 in m565. In every one the selectors move the amount, so the blur runs out as each part settles. These beliefs were measured once with `nakkash fidelity` on those samples, over all frames. The sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-17 |
|---|---|---|
| B1. The blur is drawn: each chosen part blurs by the blur times how strongly it is chosen. | Drawn with the `gaussian-blur` effect's blurriness against left out, the mean content error falls on 5 of the 8 samples with a blur above 0 and rises on none (m793, m795 and m826 hold to the fourth decimal): m802 0.1169 → 0.1163, m815 0.2623 → 0.2620, m816 0.0922 → 0.0916, m648 0.0482 → 0.0481, m760 0.0180 → 0.0179. | 8 samples |
| B2. The two numbers blur across and down apart. | m793's blur of 0 across and 90 down is the corpus's only one that differs; the score does not tell it from a blur both ways, the parts being small and blurred only while they enter. | Not measured apart |

Other things we take without evidence:
- **How far a unit blurs.** We take the blur in the unit of the `gaussian-blur` effect's blurriness (a standard deviation of 0.3 of it). Measured at 1, 1.667 and 3.333 times that, the mean content error keeps falling on the samples blurred by 113 (m815 0.2620, 0.2619, 0.2613; m816 0.0916, 0.0914, 0.0912), while the failing share rises on m816 (63.29 %, 63.41 %, 63.54 %), m793 and m826. Neither is decisive, so the unit is the effect's until a reference frame is measured at the pixels a blur alone moves.
- **What a blurred part is blurred with.** Each group of glyphs that shares a state is blurred in a buffer of its own, so neighbouring parts blurred alike do not bleed into one another's blur. The references do not show the difference at these sizes.

## Kerning

A character style's kerning (`kerning` in the document's style: metrics, optical or none) becomes the `kerning` style key ([scene format](scene-format.md#text)): metrics as `font`, none as `none`. Measured once against the stored layouts (the [composed-line cache](#the-composed-line-cache)) of the 11 samples that hold a kerning other than metrics, imported with their manifest's fonts. The sweep does not check it yet.

| Belief | Evidence | Held, 2026-11-17 |
|---|---|---|
| N1. Without kerning, each character advances by its own width: the font's kerning pairs are not applied. | The corpus's one text without kerning, m604's `mixkit.co` (6 character styles in one text), stores a line 231.45 px wide. Set with the font's kerning it is 230.80 px, without it 231.45 px, its start within 0.01 px. | 1 text |
| N2. Optical kerning is not the font's kerning scaled: it respaces each pair by itself. | Set with the font's kerning, the lines of the texts that hold optical kerning store widths 0.95 to 1.01 times ours, a different factor on each line of one text: m713 0.956 to 1.004 (median 0.971), m714 0.951 to 1.000, m715 0.947 to 1.000, m716 0.964 to 1.009. No line of m713 to m716 falls within 0.25 px. | 84 character styles in 11 samples |

Optical kerning therefore stays reported, now as `optical kerning (the font's kerning pairs are kept)`, and is set with the font's kerning, as before. A kerning the reader does not know is reported as `a kerning the scene does not have (the font's is kept)`; the corpus holds none. Over all frames, m604 and the 9 scored samples with optical kerning (m565, m595, m596, m647, m713, m714, m715, m716, m769) draw the same to the fourth decimal: `mixkit.co` moves by under 1 px.

## Skewed characters

An animator's skew and skew axis (`ADBE Text Skew`, `ADBE Text Skew Axis`) become the animator's `skew` and `skew-axis` ([scene format](scene-format.md#text-animators)). The corpus holds 17 of each in 5 scored samples (m752, m756, m791, m800, m805). All store 0, and only m756's two animators move theirs, by expressions whose bake gives a skew of 70 degrees and axes of -234 and 120 degrees. Measured once with `nakkash fidelity` on the five, over all frames. The sweep does not check it yet.

| Belief | Evidence | Held, 2026-11-17 |
|---|---|---|
| W1. A positive skew along an axis of 0 leans a part's top forward, and the axis turns the slant with it. | Drawn this way, m756's mean content error goes 0.0985 → 0.0982 and its failing share 73.68 % → 73.67 %. With the lean the other way, the error goes 0.0985 → 0.0989. The other four samples draw the same to the fourth decimal. | 1 sample, 2 animators |

Other things we take without evidence:
- **Order.** A part scales, then slants, then turns about its anchor. The corpus has no skew that turns or scales at the same time as it slants apart from m756, which does not tell the orders apart.
- **The limit.** The skew is held within -85 and 85 degrees, where its slant stays finite. No project holds more.

## Expression selectors

An expression selector (`ADBE Text Expressible Selector`) becomes a `parts` selector ([D64](decisions.md#d64-a-selector-may-choose-each-part-by-its-own-amount)): its amount's expression is evaluated once per part. These beliefs were measured once, by hand: two scenes are drawn at the reference's own times and compared with the reference only at the pixels where they differ. The sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| E1. `textIndex` counts the parts from 1, and `textTotal` is how many there are; the expression's amount, in percent, chooses its part. | Every part given the amount of the one before it, as an index from 0 would give: m554 errs 0.666 against 0.315 as baked (32,000 px), m805 0.390 against 0.094 (159,000 px). Against the selector left out, as before: m805 0.067 against 0.427 (424,000 px). | 2 samples |

What is left open:
- **One number.** An expression that gives one number, such as `if (textIndex == n) {100} else {0}`, chooses its part by it along every axis, as an open renderer reads it. The samples measured give two or three numbers, so this is taken without evidence.
- **Axes.** The amount holds three numbers. The scene takes the first for every value of one number (opacity, rotation, colours, tracking) and each axis its own for offsets and scales, as an open renderer does. On m618, whose keyed amounts run from `[100, 100, 100]` to `[-100, 0, 100]` under a position and an opacity, giving every value the second number errs less at the pixels it moves (0.205 against 0.224, base 0.208), and the first number everywhere much more (0.351). One sample does not settle it.
- **After other selectors.** In m619 and m809 the expression selector follows a range selector, whose choice it replaces and reads as `selectorValue`. These stay reported.
## Merging outlines

A merge paths group (`ADBE Vector Filter - Merge`) becomes a `merge` item ([D65](decisions.md#d65-a-merge-replaces-the-outlines-above-it-by-one)). The 14 corpus samples that use one hold 132 such groups. These beliefs were measured once, by hand: each variant is drawn from the same imported scene and compared with the reference frame only at the pixels where the variants differ (the mean of the largest channel difference, 0 to 255). The sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| M1. `ADBE Vector Merge Type` is 1 for merge, 2 add, 3 subtract, 4 intersect and 5 exclude intersections, the order of the editor's menu; a group that stores none has mode 1. | A menu property at its default is not stored: in the dumps of the 14 samples, `Fill Rule`, `Stroke Line Cap`, `Stroke Line Join`, `Star Type`, `Shape Direction` and `Composite Order` never store 1, while `Merge Type` stores 2 (3 times), 3 (56), 4 (30) and 5 (1), and never 1. m619's YouTube logos cut letters made by inner merges that store none: read as mode 1, the counter of the `o` errs by 6.3 at the 126 pixels the two readings change, read as 2 by 132.0 (frames 13 to 20). | 132 groups; 31 store none |
| M2. A merge takes every outline above it in its group, those of inner groups through their transforms; the paints listed before it, in its group or in those groups, no longer draw them. | m712's matte is two filled ellipse groups, a subtracting merge and a fill. With the fills above it drawing nothing, m712 fails on 4.73 % over all frames; with them drawing, on 13.64 %, as before merges were read. At frames 11 to 13, where the two differ, they err by 2.2 to 7.0 against 217.8 to 230.8. m817's cloud is 10 or 11 ellipse groups, each stroked and filled, then a merge that adds them, a stroke and a fill: frames 3 to 7 err by 14.5 to 21.8 against 41.2 to 110.4 where the inner strokes still draw. | 4 groups of m712, 3 of m817; m817's frame 2 errs by 26.1 against 22.8 |
| M3. The first outline is the one the others combine with: `subtract` keeps what the first covers and no later one does, `intersect` what all cover. | m712's ring keeps the first ellipse less the second (above). The intersecting merges of m594, m618 and m651 bring them closer: 79.62 % to 79.35 %, 20.56 % to 19.28 % and 97.00 % to 96.96 % failing over all frames. | 4 groups of m712, 30 intersections |

Other things we take without evidence:
- **How an outline is read.** Each outline a combining mode takes is read by the nonzero rule, whatever the fill after it says, and the result is simple contours, so the fill's rule no longer matters. This follows the open-source players that draw merges; no check against the corpus tells the two rules apart yet.
- **Trims.** No group of the corpus holds both a trim and a merge. A trim before a merge cuts what it takes, and one after cuts what it leaves, by list order like any modifier; the merged contours start where the boolean sweep starts them.
- **Exclude with more than two outlines.** The one exclusion of the corpus, m648's, has two outlines, and its reference is drawn by an extruding 3D renderer the scene does not have. With more, `exclude` combines in turn, so a point inside three outlines is kept; "the union less where they overlap" would drop it.
- **m631, m642 and m809.** m631's 38 half circles are an ellipse less a rectangle, but its reference shows other footage at those moments. m642 fills its merge with a gradient the scene does not draw yet. m809's merges change no pixel at the sampled frames.

## Round corners

A round corners group (`ADBE Vector Filter - RC`) becomes a `round-corners` item ([D66](decisions.md#d66-round-corners-cut-sharp-corners-back-along-their-sides)). The 3 corpus samples that use one, m548, m550 and m594, hold 17 such groups; the corners the measurements below cover all meet square. The beliefs were measured as those on merging outlines were: variants of one imported scene, compared with the reference at the pixels where they differ.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| R1. A group that stores no `ADBE Vector RoundCorner Radius` has radius 10. | m550's audio bars store none. Drawn with radius 10 they err by 22.7 to 24.9 at the 413 to 426 pixels the radius changes, with 0 by 61.2 to 65.9, with 5 by 49.4 to 52.7 and with 20 by 85.1 to 90.5 (frames 4 to 9). | 2 groups of m550 |
| R2. The stored radius cuts each square corner back by that many px along both sides, at most half of each side, and the cut points are joined by a quarter circle. | m548's bars store 4 and 10. As stored they err by 32.9 to 37.9 at the 494 to 533 pixels the radius changes, sharp by 50.1 to 53.0 and doubled by 60.4 to 74.5 (frames 5 to 12); at frame 4, on 136 pixels, doubled errs least (35.4 against 46.7), and the share of those pixels past 25 is lower sharp (49 to 51 %) than as stored (61 to 71 %). | 6 groups of m548 |

Other things we take without evidence:
- **Corners that do not meet square.** Every rounded corner measured here is a right angle, where cutting back by the radius and drawing a circle of that radius are the same. The scene cuts back by the radius whatever the angle, so a sharp corner gets a shorter arc than a circle of that radius would.
- **Which vertices count as corners.** Only a vertex with no handle on either side is rounded. m594 rounds the tip of a heart whose neighbours each have one handle; its hearts are not on screen at the sampled frames.
- **The handle length.** 0.5519 of the cut, against the 0.5523 a quarter circle needs: the two differ by less than a hundredth of a pixel at these radii.

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

## What `sourceRectAtTime` reads

An expression reads the rectangle a layer's content covers, `{left, top, width, height}` in the layer's own space, by `sourceRectAtTime(t, includeExtents)`. The import measures it in the scene it writes, with the code that draws: the text set in its fonts, the shapes drawn (`nakkash_engine::measure_layouts`, `nakkash_scene::content_rects`). The sources disagree on what it is. The editor's expression reference takes a time (default the current one) and `includeExtents` (default false), which applies only to shape layers, whose bounds grow to take in their strokes, and to box text, which then gives its box. An open-source web player of exported animations measures the element it has drawn at the frame it is on (the browser's box of the text as rendered, animators and all), ignores both arguments, and does not measure shape layers at all (it answers a fixed 100 by 100); the community reference of that export format lists no arguments. These beliefs were measured once, by hand, against the corpus's reference frames, by setting the boxes our render draws beside the reference's; the sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| R1. Text covers the outlines of its glyphs: not the font's ascent and descent, nor the advance of a trailing space. Tracking counts where it moves glyphs. | m543 and m602 size chat bubbles to their text's rectangle and pad them by an offset of 23 px (which the scene does not draw yet). Each reference bubble is our rectangle plus 44 to 49 px both ways, with its centre within 1.5 px of ours, whatever its text: one-line bubbles whose words have a descender are 92 to 94 px tall in the reference, `Welcome to the Mixkit Crew!`, which has none, 85 px; ours 48 and 39. Ascent and descent would make every one-line bubble one height. m602's first bubble ends its text with a space: it comes out no wider against ours than its neighbour does (6 to 8 px narrower, against 5), where the space would add 12 px. Both samples set their text with tracking −10. | 33 bubbles in 10 frames of m543 and m602 |
| R2. Box text covers its words, not its box; `includeExtents` gives the box. | m624, m806, m808 and m813 frame callouts with the rectangle of text set in a box 2,086 by 1,034 px, m636 with one 1,864 by 362 and m778 centres its anchor on one 1,137 by 274: in every reference the frame hugs the words. None asks for extents. | 6 layers in 6 samples |
| R3. Animators change nothing: a character an animator moves or hides covers where it is set. | m586 sizes its box to its text at the current time while an animator reveals the text, each hidden character 100 px lower and transparent. At 1.0 s, when only `BOX` shows, and at 1.5 s, with `HERE` still hidden, the reference box spans the same 590 by 321 px as once all three lines show. Measured on the visible glyphs, the box would grow with the reveal; on the glyphs where the animator puts them, it would reach 100 px lower. | 1 layer, 9 frames |
| R4. Centring a layer's anchor on its rectangle centres its ink. | m748 centres its words' anchors on their rectangles: their ink's centre lies at y = 539.5 px in ours as in the reference (502 before, with the anchor kept). m778's title lies at (541.5, 884) in ours and (539.5, 883.5) in the reference (y 844 before). | 4 frames of m748, 1 of m778 |

What is left open:
- **Shapes and extents.** No local sample reads a shape layer's rectangle except m586 its own, in an expression that fails on `toComp` first, and none passes `includeExtents` true: of the 131 calls in the 241 local projects, 130 read text. A shape's rectangle covers its outlines where its groups put them, and with extents half each stroke's width, scaled by its groups, as the editor's reference says, unmeasured.
- **Other layers.** A layer with a source covers its source's box from the origin, as both sources say; no local sample reads one. A camera or a light is reported.
- **Empty text** covers an empty rectangle at the origin, unmeasured. So does text whose every glyph is a space.
- **A text stroke** is left out of the rectangle; the strokes of the local samples that read one are 1 px or less.
- **What the layout does not match.** The rectangle is our layout's; where it differs from the stored one ([the composed-line cache](#the-composed-line-cache)), the box differs with it. m602's white bubbles come out 5 to 8 px narrower in the reference than ours with the padding, its green ones within 3 px.
- **A property's own rectangle.** A layer whose content reads its own rectangle is measured with the expressions that wait drawn as the keys under them: what we take the original to read for a property while its own expression runs. m586 is the only such layer, and its expression fails first.

## Expanded outlines

Offset Paths (`ADBE Vector Filter - Offset`) becomes an `expand` item ([D68](decisions.md#d68-an-expansion-moves-outlines-out-by-an-amount)). Every one in the corpus grows a rectangle: 44 items in 10 samples, 40 of them storing an amount (23, 28, 45, 72 or 0) and nothing else, and 5 in m566 storing nothing. No local project of the 206 stores a line join, a miter limit, copies or a copy offset. These beliefs were measured once, by hand, on the reference frames; the sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| O1. A side moves out by the amount along its normal, so a rounded corner of radius `r` grows to `r + amount`. | m543 rounds its speech bubbles by 20 and grows them by 23. Fitting a circle to the left corners of the three whole blue bubbles of reference frame 13 gives radii 43.0, 41.5 and 43.5 px (rms 0.6, 0.7 and 1.2 px). | 3 bubbles of m543 |
| O2. The miter limit counts in amounts from the corner, as a stroke's counts in widths: at the default 4, a right-angled corner is mitred, square. | m624 grows its title bar, a sharp rectangle, by 28 under the default limit, and scales it by 0.93 by 0.11. The bar's rows in reference frames 3, 12, 16 and 20 all start at x = 2,503 or 2,504, its first and last ones too: square corners. A limit of 4 px, as one reading of the original's behaviour has it, would bevel them, cutting about 24 px from the first and last rows. | m624; m806, m808 and m813 use the same rig |
| O3. An Offset Paths that stores no amount grows by 10. | m566 stores none on its 5 items; the editor's default. | not measured: m566's boxes are sized by expressions the bake cannot run yet |

Other things we take without evidence:
- **Direction.** A positive amount moves a side to its left, so outward on a clockwise outline, and inward on a counterclockwise one. Every expanded outline in the corpus is a clockwise rectangle.
- **Space.** An expansion after a group grows that group's outlines in their own space, as a trim measures lengths. m624 grows a rectangle in a group whose scale animates, but its size and anchor come from expressions on the text's size, which the bake cannot run yet, so the frames cannot tell the two spaces apart.
- **Open outlines, crossing sides and copies.** No sample expands an open outline, a concave one, or makes copies. An open outline becomes the closed one around it, sides that cross are cut where they cross, and copy `k` moves `amount · (k + copy-offset)`.
- **The default match names** of the line join (`ADBE Vector Offset Line Join`), miter limit (`… Miter Limit`), copies (`… Copies`) and copy offset (`… Copy Offset`), which no local project stores.

## What expressions read

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| X1. An expression reads a 2D layer's anchor point, position and scale as two components, though the project stores three; a 3D layer's, a camera's and a light's as three. | A gaming lower-third family (m571 to m575, m739, m783) sizes its logo with a rig on a 2D null: its parent's scale expression gives `[thisComp.width, thisComp.height]`, two components, and the child divides each component of its own scale by its parent's, `for (i = 0; i < s.length; i++)`. With three components the third is `NaN` and the original would show an error; with two the logo's world scale is 2 by 2, and the logo lies on the reference's: m783 1.72 % → 0.93 % failing, m571 7.40 % → 1.85 %, m573 9.17 % → 1.78 %. | Over 42 corpus samples under 25 % failing, 5 draw closer and none farther. |

## Beams

`ADBE Laser` becomes the `beam` effect ([scene format](scene-format.md#shapes-masks-effects-and-compositions)). Its parameters by match name: `-0001` Starting Point and `-0002` Ending Point, effect points like any other; `-0003` Length and `-0004` Time, fractions from 0 to 1 (the definition's sliders, defaults 0.25 and 0.5); `-0005` and `-0006` the thickness at the start and at the end, in px; `-0007` Softness, 0 to 1; `-0008` Inside and `-0009` Outside Color; `-0010` 3D Perspective (on by default) and `-0011` Composite On Original (off by default), two checkboxes the definition leaves unnamed, told apart by their defaults and by m587 to m590, whose projects name `-0010` "3D Perspective". 20 local samples hold 101 lasers; the beliefs were measured once, by hand, against the references of the 11 that draw one. m622, m624, m808 and m813 are 3840 by 2160 compositions measured against 1080p previews scaled up, so lines a few px thick come out wider and softer in the reference.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| B1. The beam's front is `time × (1 + length)` of the way from the start to the end point, and its back is `length` behind the front, both kept to the line. With Length 1, Time 0 draws nothing, 0.5 the whole line, and 1 nothing again. | The call-outs of m622, m623, m624, m806, m808, m813 and m848 key Time from 0 to 0.5 with Length 1, and the corpus's static rigs (m571 to m575, m625, m779, m783) leave Time at its default of 0.5 with Length 1. Both make sense only if 0.5 draws the whole line. m622's underline runs from (2612, 1480) to (3544, 1480). Settled, the reference draws it from x 2598 to 3546 and we draw it from 2599 to 3543. Read with the front at `time` of the line, it would stop at 3078. m586 to m590 key Time from 1 down to 0.5; m589's settled reference draws the whole line from the text box to the dot, as we do. | 11 samples drawing a laser |
| B2. A laser draws in place of its layer: its solid does not show. | m589 and m622 put their lasers on white 1920 by 1080 or 3840 by 2160 solids; their references are dark around the lines. | 11 samples |
| B3. The thickness is in px of the layer. | m622's lines, 4 px by their controls, are 4.0 px wide drawn so against 4.4 px in the reference, measured across the diagonal line as the light summed over 15 rows. The 0.4 px is in line with the blur that scaling up the 1080p reference adds. | 1 sample |

Other things we take without evidence:
- **Length below 1.** Only the `Text Box` lasers of m587 to m590 use another length (0.5), on text layers behind `CC Composite`, which the scene does not draw. The scene gives a text layer no box to keep the beam to, so those beams draw as tall strips rather than boxes behind the text (m589's error rises 0.2867 → 0.3141).
- **Softness and colours.** Every local laser has Softness 0 and the same inside and outside colour, after expressions. The scene fades the edges and blends from the inside to the outside colour as the audio effects, which share the parameters, do.
- **Unequal thicknesses.** Every local laser draws the same thickness at both ends after expressions; the scene runs it straight from one to the other along the line.
- **3D Perspective and Composite On Original.** No sample shows what 3D Perspective changes; the scene draws the beam flat and reports a laser that turns it off. No sample turns on Composite On Original; the scene keeps the layer under the beam when it is on.

## Effects that draw in place of their layer

Some generators draw something sparse of their own, particles, stars, a beam, from their layer and keep none of its pixels. While the scene cannot draw one, the import draws its layer as nothing, rather than the solid the effect covers: the `Policy::InPlace` effects of the import's policy table, reported as unsupported with the layer. Generators that fill their layer (gradients, noise, cells) keep their solid as the nearer stand-in, and effects that draw over their layer's pixels (a lens flare) keep them. The beliefs were measured once, by hand, against the reference frames; the sweep does not check them.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| P1. `CC Star Burst` draws its stars in place of its layer: no pixel of the layer itself shows, whatever its colour and opacity. | m827, m845 and m846 lay `Skyback`, a white 3840 by 2160 solid at 40 % opacity holding only Star Burst, over `Sky`, a navy ramp. The references' background is (0.020 to 0.024, 0.035, 0.059 to 0.063) in m827's frames 1, 11 and 20; drawing the solid gives (0.416, 0.420, 0.439) there, drawing nothing (0.024, 0.035, 0.063). m774's `stars basckground` is an opaque white 1920 by 1080 solid with Star Burst first; its reference is black behind the title at 5 s and 9.5 s, where the solid drew the frame white. | 4 samples, every one holding the effect |
| P2. What follows such an effect works on what it draws, so it goes with it. | m774's stars layer runs `CC Light Burst 2.5`, `ADBE Matte Choker` and `CC Toner` after Star Burst; m563 runs Light Burst after `CC Particle Systems II`. Light Burst streaks what is drawn before it, a filter, not a generator: drawn as nothing with the stars, m774's frame at 5 s errs by 0.069 a channel against 0.921 with the solid. | 2 samples |

Other things we take without evidence:
- **The other `InPlace` effects.** `ADBE AudWave`, `CC Particle World`, `tc Particular` and `tc Form` draw on a transparent layer by default, as their documentation says. Nine samples hold Particle World; their references were not compared one by one. `ADBE Laser`, which was one of them, is now drawn: see [Beams](#beams).
- **A composite-on-original switch.** Several of these effects can keep their layer's pixels under what they draw. No local sample turns one on; a project that does draws nothing where it should draw its layer.

## Inverted fills

`ADBE Fill` turns what its layer draws one colour. Its `Invert` checkbox (`ADBE Fill-0006`) becomes the fill effect's `inverted` setting ([scene format](scene-format.md#shapes-masks-effects-and-compositions)). Two local samples turn it on, both with no mask chosen (`Fill Mask` 0) and full opacity. The belief was measured once, by hand, against their reference frames; the sweep does not check it.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| F1. Inverted, with no mask chosen, the fill takes the layer's transparency as its coverage within the layer's box: it fills where the layer draws nothing and clears where it draws. Where the layer is opaque, nothing shows. Per premultiplied pixel, the colour at `1 − a`, mixed with the layer by the fill's opacity. | m651 lays `White Solid 1`, an opaque 3840 by 2160 solid with an inverted fill of the default red, under its player interface. The reference is black behind the interface (mean 0.005 a channel at 3.5 s and 7 s), not white (the solid kept) or red (the fill drawn as it is, our frame at 3.5 s half red). m649's `Adjustment Layer 1`, an adjustment layer at the bottom of its composition with an inverted white fill, has nothing below it, so it adjusts a transparent frame. Its reference is white from the first frame. Read this way, m649's frames at 0 s, 3 s and 5.5 s differ from the reference by 0.0000, 0.0009 and 0.0023 a channel, and m651's at 3.5 s and 7 s by 0.0010 and 0.0009. Read as a fill that leaves the layer as it is, both stay as far off as before (black for m649, white for m651). | 2 samples, every one holding an inverted fill |

Other things we take without evidence:
- **Partial alpha and opacity.** Both samples fill fully transparent or fully opaque pixels at full opacity. The mix at an edge's partial alpha and below full opacity follows the plain fill's.
- **Layers without a box.** A shape or text layer has no box in the scene, so its inverted fill covers the whole frame. Its box in the original is the composition's size, in the layer's own coordinates; no sample shows where they differ.
- **Masks.** `Fill Mask` and `All Masks` are still only read at their defaults; an inverted fill of a chosen mask is reported, as before.

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

## Layers that run backwards

A layer's in and out points (`ldta`) say when it shows. These beliefs say how a layer whose stretch is negative, one that plays its source backwards, stores them. They were checked by hand over every local sample; the sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-17 |
|---|---|---|
| W1. A layer with a negative stretch stores its in point after its out point, and shows between the two: from the out point to the in point. Its start time is where its own time is 0, as for any layer. | m821's `Text Animation` holds `Title Texts` twice: once from 0 to 2.46 s at stretch 1, and once with in point 4.92 s, out point 2.46 s, start 4.92 s and stretch −1, the title playing out backwards. The reference shows the title from 2.5 s to 4.0 s, a white `MIXKIT SPORTS TITLE` our frames lacked (mean −10.8, −7.6, −6.5 levels a channel over the whole of those frames). | 28 layers in 12 samples: every layer with a negative stretch, and no layer with a positive one, stores its in point after its out point |

Before, the import took such a layer for one that never shows and wrote it hidden, with nothing reported. Over all frames:

| Sample | Failing | Content error | Frames passing over the floor |
|---|---|---|---|
| m635 | 68.64 % → 66.21 % | 0.2671 → 0.1657 | 3 → 3 of 11 |
| m830 | 33.02 % → 29.43 % | 0.0307 → 0.0155 | 0 → 0 of 20 |
| m837 | 37.75 % → 35.00 % | 0.0370 → 0.0235 | 0 → 0 of 20 |
| m847 | 21.55 % → 20.49 % | 0.0139 → 0.0087 | 0 → 0 of 19 |
| m821 | 88.00 % → 86.75 % | 0.1041 → 0.0955 | 1 → 1 of 20 |
| m794 | 41.17 % → 40.91 % | 0.0191 → 0.0171 | 0 → 0 of 20 |
| m594 | 79.35 % → 79.28 % | 0.0464 → 0.0454 | 1 → 1 of 20 |
| m796 | 31.11 % → 30.78 % | 0.0378 → 0.0375 | 0 → 0 of 14 |
| m593, m631, m782, m784 | unchanged | unchanged | unchanged |

m821's levels, which hide its colour, are read in another change; with both, its error falls further.
## Effects that read a collapsed composition

An effect parameter that names a layer (`pard` control 0) reads that layer's drawing in its own coordinates, without its transform ([scene format](scene-format.md#shapes-masks-effects-and-compositions)). This belief says what it reads of a precomposition layer whose transformations are collapsed. It was measured by hand on one sample; the sweep does not check it.

| Belief | Evidence | Held, 2026-11-17 |
|---|---|---|
| K1. A collapsed precomposition read by an effect is its composition in its own coordinates, as if it were not collapsed: collapsing changes where it draws, not what the effect reads. | m785's `Adjustment Glitch` layers displace the frame by `Noise Precomp 1`, a hidden, collapsed 3840 by 2160 composition of block noise 7.65 % high, scaled to 0.047 by 0.084 under a parent. Its collapsed layers carried the comp layer's transform, and the read laid them on the reading layer's coordinates once more, so the map read as one level almost everywhere: the frame moved as one by the full displacement (164 px across at 0.4 s, every row alike but at the frame's edges). The reference cuts the frame into thin horizontal slices, each moved its own way; read as its composition, ours does too. | 4 local samples read a collapsed composition, all by `ADBE Displacement Map`: m577, m785, and m598 and m601, excluded |

Over all frames, m785 fails on 49.67 % → 45.67 % of its content (content error 0.0608 → 0.0431) and m577's content error goes 0.4441 → 0.4320; m598 and m601 do not move, and no sample passes more frames. m785 still differs in its glitch frames: there the reference shows the clip further along (the runner smaller, nearer the sea) than the scene's clip at the same time, which this change does not explain.
## Inverted noise

`ADBE Fractal Noise-0003`, the noise's invert checkbox, becomes the `fractal-noise` effect's `polarity` (`inverted`). The belief says where the inversion falls among the level's steps. It was measured once, by hand; the sweep does not check it.

| Belief | Evidence | Held, 2026-11-18 |
|---|---|---|
| N1. The level is inverted first, and contrast, brightness and overflow then apply to the inverted level. | A third-party project's background stacks an inverted noise at contrast 1.73 and brightness −0.39 under a curve that sends every level above 0.63 to white. Its reference stills are dark, with bright thin streaks over the whole frame. Inverting last turned the darkened field over into a white one with dark streaks, which the curve then sent to white everywhere. Inverting first gives the reference's dark field and bright streaks. m569, the one corpus sample with an inverted noise, keys its brightness: where our layer draws, the content error falls from 0.6954 to 0.2370 over its 8 scored frames; over the whole frame it moves from 0.7764 to 0.6644. | 1 corpus sample and 1 project outside the corpus |

## Levels

`ADBE Easy Levels2` keeps its levels in its histogram parameter (`-0002`), as data: 108 bytes in every local sample. The effect definitions the projects carry mark that parameter animatable and the sliders (`-0003` to `-0007`) and the channel menu (`-0001`) not. These beliefs say how the data reads and how the effect applies it. They were checked by hand over every local sample; the sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-17 |
|---|---|---|
| L1. The data is a 4-byte head (`00 0F 10 A7`), a big-endian `u32` (1 in every sample, the panel's channel by our reading), then five sets of five big-endian `f32`: input black, input white, gamma, output black, output white, levels from 0 to 1, for RGB, red, green, blue and alpha in that order. | Every value read lands in its range and the gammas are positive; the sets read as typed levels (0.1373 is 35/255, 0.0941 is 24/255, 0.8431 is 215/255, 0.7882 is 201/255). | 10 effects in 7 samples |
| L2. Each channel's set applies first, then the RGB set to red, green and blue, as curves apply theirs: a level becomes `((c − black) / (white − black))^(1/gamma)` clipped, then spread from output black to output white, on the values as stored. | m821's `Color Correction` holds RGB (0.1373, 1, 2.1089, 0, 1), red (0.0667, 1, 1.1399, 0, 1) and blue (0.0941, 1, 1.3282, 0, 1). On the flat regions of reference frame 7 (3.0 s), binned by our level before the effect, the reference's median sits within 4 levels of this model in every channel from 42 to 162 (green 54 → 84, model 80; 102 → 147, model 145; blue 102 → 158, model 156). Against the whole frame the mean error is 11.1 levels with each channel first and 12.4 with RGB first; with the stored sets left out it is 42.3. | m821; m591, whose only sets are red (0.0471, 0.9961, 1.0995, 0, 0.9725) and blue (0.0118, 1, 1, 0.0745, 0.9647), and m820, m839 and m551 move as the model says (below) |
| L3. The sliders and the channel menu change nothing drawn: they show one set of the data, as the panel last showed it. | m821's first `Color Correction` stores sliders named `Red Input Black` … `Red Output White` at 0, 1, 1, 0, 1, while its data's red set is (0.0667, 1, 1.1399, 0, 1) and its reference follows the data (L2). The other nine effects store no slider at all, and their data's sets are not all identity. | 10 effects in 7 samples |

Before the data was read, the import took the sliders, so all ten effects drew as identity and nothing was reported. Reading it, over all frames:

| Sample | Failing share | Content error | Frames passing over the floor |
|---|---|---|---|
| m821 | 88.00 % → 29.83 % | 0.1041 → 0.0173 | 1 → 1 of 20 |
| m820 | 66.00 % → 40.47 % | 0.0771 → 0.0678 | 0 → 0 of 20 |
| m839 | 50.50 % → 25.68 % | 0.0323 → 0.0182 | 0 → 0 of 20 |
| m551 | 18.20 % → 16.23 % | 0.0185 → 0.0169 | 0 → 0 of 11 |
| m591 | 94.92 % → 71.85 % | 0.1221 → 0.1071 | 1 → 1 of 20 |
| m558, m738 | unchanged | unchanged | unchanged |

m558's levels sit on a hidden noise layer. m738's darken two plates of its `_Camera` composition (output white 0.7882 on one, identity on the other); its numbers do not move to the fourth decimal.

What is left open:
- **The head and the `u32`.** Every sample stores the same head and 1; data with another length still reads its first 108 bytes.
- **Alpha.** Every sample's alpha set is identity; an alpha set applies to straight alpha as the scene's `levels` does, unmeasured.
- **Keys.** No sample keys the histogram; a keyed one keeps its first key and is reported.

## Colours

A colour property stores four channels, its alpha among them. This belief says what the original draws of that alpha. It was checked by hand over every local sample; the sweep does not check it yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| C1. The original draws every colour it stores opaque: the alpha a colour carries, stored or given by an expression, changes nothing drawn. Where a colour can be see-through, an opacity of its own says so (a fill's opacity, a shape fill's). | m713, m714 and m716 colour their shapes and `ADBE Fill` effects by expressions that read colour controls storing alpha 0, so the fills, strokes and dashes they drive come out at alpha 0: 72 in m713, 44 in m714, 17 in m716. Their references show every one of them opaque. Written opaque, m713 goes from 67.84 % to 5.60 % failing over all frames, m714 from 87.70 % to 3.48 % and m716 from 29.72 % to 5.36 %; the 24 other samples whose scenes change score the same. | 10,982 stored colours under 122 match names in the 198 samples; the 145 with alpha below 1 are all effect parameters and colour controls: `ADBE Fill-0002` 48 (each driven by an expression), `ADBE Tint-0001` and `-0002` 77, `ADBE Laser-0009` 7, `ADBE PhotoFilterPS-0002` 1, `ADBE Color Control-0001` 12. No reference shows a colour see-through by its alpha. |

What is left open:
- **Text.** A text run's fill and stroke come from its document, not a colour property; a run with neither is written clear, as before.
- **Effects the scene does not draw yet.** The beams of `ADBE Laser` and Photo Filter store alphas below 1; they will draw opaque when mapped.


## Repeaters

A repeater (`ADBE Vector Filter - Repeater`) becomes a `repeater` item ([scene format](scene-format.md#shapes-masks-effects-and-compositions)). 13 local samples hold 65 of them, all at the end of their groups, after paths, paints, trims, zig zags or twists, and two of them twice in a row. None stores an offset, an anchor or a composite, and none animates the count. The beliefs were measured once, by hand: each sample's scene is drawn under two readings, and both are compared with the reference only at the pixels where the readings differ (more than 4 levels on a channel, premultiplied), as the mean of each pixel's largest channel error. The sweep does not check them yet.

| Belief | Evidence | Held, 2026-11-16 |
|---|---|---|
| R1. Copy `k` of `n` has the opacity `start + (end − start) · k / n`: the last copy stops one step short of End Opacity. | m647 fades 3 copies to an End Opacity of 0 and m769 the same rig with keyed opacities. Against `k / (n − 1)`, which would hide m647's last copy, `k / n` errs by 0.0368 against 0.0802 over 205 862 pixels of 6 frames of m647, and by 0.0145 against 0.0334 over 42 381 pixels of 16 frames of m769; it errs less on every frame where they differ. | 2 samples, 22 frames |
| R2. The repeater's match names: `Copies`, `Offset`, `Order` (the composite) and the group `Transform`, holding `Anchor`, `Position`, `Scale`, `Rotation`, `Opacity 1` (start) and `Opacity 2` (end), each after `ADBE Vector Repeater`. | The 129 repeaters in the 206 local projects store Copies, Position, Scale, Rotation and both opacities under these names, in the Transform group. Offset, Order and Anchor are stored by none; their names are the published ones. | 129 repeaters |
| R3. A repeater that stores no Position holds it at the origin. The editor gives a new repeater a position of [100, 0] and writes it, so a stored [100, 0] is that default. | 112 of the 129 store a Position; the 17 that do not turn their copies (m564's 5 lines at 73°, m565's rings at `360 / copies`, m631's at 60°), as a ring about the anchor does. Drawn with the origin, m564 errs by 0.3145 against 0.4552 for [100, 0] (19 036 pixels) and m565 by 0.3368 against 0.4385 (22 876). m631 ties (0.6394 against 0.6438), its reference far off elsewhere. | 2 samples |

Other things we take without evidence:
- **Steps add up, they do not compound.** Copy `k` moves by `k` positions and turns and scales about the anchor by `k` steps, as both external readings of the original do; a copy's move is not turned by the copies before it. The samples that move and turn or scale at once, m607 and m608, are transitions laid over stock footage the projects leave out, and their references cannot tell the readings apart (m608: 0.5033 against 0.5053 for compounding steps).
- **Scale is a power.** Copy `k` is scaled by Scale to the power `k`. m607's 20 copies at 105 % and m608's 3 at 91 % or 47 % are where it would show, behind the same footage.
- **Composite.** `Order` 1 is `below`, each copy under the one before, so the first is on top; 2 is `above`. Every sample is at the default, and flipping the order changes no pixel of m574, m607, m608, m647, m760 or m769: their copies do not overlap in colour.
- **Fractional counts.** None in the corpus. The scene draws one copy more, its opacity times the fraction, which is how the editor's users describe an animated count fading copies in; one external reading rounds the count, another takes its ceiling without a fade.
- **Fractional and negative offsets.** None in the corpus. The scene takes the offset as more or fewer steps, fractional steps included, with Scale's power and a turn in proportion; it does not change the fade.
- **Items after the repeater.** None in the corpus: a paint or trim after a repeater in its group reaches every copy's outlines in the scene, as the items after a group reach the group's. One external reading copies only the paints before the repeater, leaving its outlines to later paints once.
- **Copies.** Every repeater stores its count, so the default count, 3 for the scene as for a new repeater in the editor, is not measured.

## Vector footage

Footage in a PDF file, which every `.ai` file of the corpus is inside, becomes an image layer drawn from its outlines ([D70](decisions.md#d70-vector-footage-is-drawn-from-its-outlines)). 11 files in 10 local samples: one logo in m562, m593 and m595, another in m563 and m564, and m712, m716 (two), m794, m803 and m830 one each. All have one page, paths, fills and strokes, clips, ICC-based or device colour and normal-blend graphics states; none has text, images, shadings, patterns or soft masks, so the tests of those families are hand-written PDFs. Measured once, 2026-11-17, against a build of `main` (90def20), each sample's 20 frames or fewer under the floor rule ([D69](decisions.md#d69-a-reference-frame-passes-over-its-codec-floor)).

| Sample | Failing, before → after | Mean error, before → after | Frames passing over the floor, before → after |
|---|---|---|---|
| m562 | 0.64 % → 0.56 % | 0.0022 → 0.0012 | 13 → 13 of 13 |
| m563 | 23.77 % → 23.05 % | 0.0289 → 0.0208 | 2 → 8 of 13 |
| m564 | 7.88 % → 7.20 % | 0.0154 → 0.0075 | 1 → 6 of 11 |
| m593 | 43.53 % → 43.46 % | 0.0627 → 0.0621 | 1 → 3 of 20 |
| m595 | 72.66 % → 72.66 % | 0.0964 → 0.0964 | 1 → 1 of 20 |
| m712 | 4.72 % → 4.72 % | 0.0117 → 0.0117 | 10 → 10 of 20 |
| m716 | 5.36 % → 5.36 % | 0.0145 → 0.0145 | 5 → 5 of 20 |
| m794 | 41.21 % → 41.20 % | 0.0198 → 0.0197 | 0 → 0 of 20 |
| m803 | 77.49 % → 77.48 % | 0.0947 → 0.0923 | 0 → 0 of 20 |
| m830 | 36.22 % → 33.02 % | 0.0460 → 0.0307 | 0 → 0 of 20 |

m712's and m716's vector layers are hidden (guides), so they cannot change; m595's numbers do not change either, which is not yet explained. m794 also holds an `.eps` file, drawn since [D71](decisions.md#d71-eps-footage-is-drawn-by-our-own-postscript-interpreter) ([EPS footage](#eps-footage)).

| Belief | Evidence | Held, 2026-11-17 |
|---|---|---|
| V1. A vector file's footage size is its crop box rounded out to whole units, or, when the project imported it to its art, its art box rounded out. The layer shows that box at one pixel a unit. | Nine of the eleven are their crop box exactly (1440 by 900, 1080 by 1080, 190 by 36); m794's hearts are 46 by 30 on a 45.16 by 29.08 page; m830's footage is 444 by 662 on a 1920 by 1080 page whose art box, [738.14, 209.59, 1181.14, 870.59], rounds out to x 738 to 1182 and y 209 to 871. Drawing m830's art box errs 0.0307 over its 20 frames; its page stretched over the box, 0.0440. m794 cannot tell its page drawn one to one from stretched (0.01966 against 0.01968). | 11 files |
| V2. Without the layer's switch, vector footage is drawn at its footage size and the picture transformed; with it, it is drawn through the transform at the output's resolution. | m803 scales its ornament 5.34 and 3.45 times with the switch on: drawn continuously it errs 0.09235, at its box size 0.09288, lower on 18 frames and equal on 2. m563, m794 and m595 have the switch on at about their own size and cannot tell (equal to 0.00001). m562, m564 and m830, without it, shrink their footage (0.47, 0.7, 0.85 and 0.63 times), where the two readings agree to 0.00001. | 7 samples |

How close our drawing of each file is to an independent rasteriser's (the operating system's PDF renderer, run locally, never in CI), each page drawn whole on a clear background and compared premultiplied, over the pixels either covers:

| File | Mean error, 1 px a point | Failing (more than 8 levels), 1 px a point | Failing, 4 px a point |
|---|---|---|---|
| m562, m593, m595 logo | 0.00083 | 1.43 % | 0.39 % |
| m563, m564 logo | 0.00058 | 1.23 % | 0.35 % |
| m712 controller | 0.01038 | 32.74 % | 11.28 % |
| m716 geometry | 0.00024 | 0.75 % | 0.19 % |
| m716 plant | 0.00055 | 1.85 % | 0.48 % |
| m794 hearts | 0.01818 | 34.18 % | 12.90 % |
| m803 ornament | 0.02212 | 67.42 % | 35.70 % |
| m830 base | 0.00002 | 0.00 % | 0.00 % |

The files that differ most are thin line art (the controller's outline, the hearts, the ornament): the pixels that differ are their edges, by a few levels, where the two rasterisers spread coverage differently; their total coverage agrees within 1 % (the ornament 4480 against 4495 pixels at 4 px a point). Filled art agrees to under 2 % of its pixels at 1 px a point.

Other things we take without evidence:
- **Pages after the first.** Every corpus file has one page; the first is drawn.
- **Soft masks, shadings, patterns, images and text.** No corpus file has them; they follow the PDF standard, each tested on a hand-written file.
- **CMYK.** No corpus file paints in device CMYK; it goes through the interpreter's press profile. On a hand-written file the operating system's renderer, with a press profile of its own, draws 100 % magenta and yellow as (216, 34, 42) where ours is (237, 29, 36); an axial shading with a stitched function on the same file agrees within 2 levels.

## EPS footage

EPS footage is drawn by our own PostScript interpreter ([D71](decisions.md#d71-eps-footage-is-drawn-by-our-own-postscript-interpreter)). Five local samples hold EPS footage, five files in all: three plain programs (m788, m837, m838) and two binary EPS files whose programs carry half a megabyte of procedure sets (m772, m794). The beliefs were measured once, against the samples' reference renders, with the files drawn through a measuring build.

Drawn through the vector footage path, measured against a build of main with D70 (content failing share, content error, frames passing over the floor, over all frames):

| Sample | Failing, before → after | Mean error, before → after | Frames passing over the floor |
|---|---|---|---|
| m772 | 19.24 % → 18.48 % | 0.0149 → 0.0089 | 0 → 0 of 14 |
| m788 | 22.38 % → 22.34 % | 0.0130 → 0.0122 | 0 → 0 of 20 |
| m794 | 41.20 % → 41.17 % | 0.0197 → 0.0191 | 0 → 0 of 20 |
| m837 | 38.16 % → 37.75 % | 0.0392 → 0.0370 | 0 → 0 of 20 |
| m838 | 30.04 % → 29.92 % | 0.0176 → 0.0166 | 0 → 0 of 20 |

The three plain files reach what converting them to PDF outside nakkash and drawing that with the operating system's renderer reached (m788 22.3 %, 0.0122; m837 37.8 %, 0.0370; m838 29.9 %, 0.0165). m794's error is mostly its background clip; its ornaments are recoloured by a fill effect. The other samples hold their errors in footage and text the EPS files do not touch.

| Belief | Evidence | Held, 2026-11-17 |
|---|---|---|
| E1. Device CMYK is drawn by the language's own rule, each primary `1 − min(1, c + k)`, not through a press profile. | m772's button is `0 .993347 1 0` CMYK. The reference draws it (255, 3, 2), (254, 2, 1) and (255, 0, 0) at four points of frame 12. The rule gives (255, 2, 0); the system's generic CMYK profile gives (217, 37, 42). | 1 colour of 1 sample; m794's one CMYK colour is recoloured by a fill effect, so its reference cannot tell. |
| E2. The footage's size is the whole-point `%%BoundingBox`, and the art sits in it one point a pixel from its top-left corner, not stretched to it from the precise `%%HiResBoundingBox`. | Every project gives its EPS footage the whole-point box's size: m772 258 × 64 (precise 257.9711 × 63.9789), m794 78 × 24 (precise 77.3357 × 23.0674), m788 384 × 384, m837 410 × 366, m838 511 × 491. Drawn over the whole-point box, m794's content error is 0.01919 against 0.01961 stretched from the precise box (41.18% against 41.21% failing); m772's differs by under 0.001 points. | 5 of 5 files |
