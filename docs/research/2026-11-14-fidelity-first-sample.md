Status: Draft · 2026-11-14

# Fidelity of the first sample

The first sample (a 1080x1920, 25 fps story layout, see the [plan](../plans/2026-11-14-aep-to-frames.md#the-reference-sample)) imported with `motion import` and compared with its reference render by `motion diff`, over the reference's 302 frames (5 s at 59.94 fps). The reference shows a photo where the project has an empty placeholder composition, so the placeholder and the full-frame solid behind it are left out (`--ignore`).

```text
motion import sample.aep --out scene.json --font fonts/lato
motion diff scene.json --reference frames/ --fps 59.94005994 \
  --ignore black-solid-1 --ignore image-placeholder --json diff.json
```

## The numbers

Errors are mean absolute differences of 8-bit sRGB channels, 0 to 1. A pixel fails when a channel differs by more than 0.016 (Blender's render-test tolerance). "Where drawn" is where the scene draws in that frame; "over all drawn" is everything it draws in any compared frame, so a missing or late element counts.

| Time | Error where drawn | Failing where drawn | Error over all drawn | Failing over all drawn |
|---|---|---|---|---|
| 0–1 s | 0.036 | 7.1% | 0.538 | 92.9% |
| 1–2 s | 0.034 | 6.8% | 0.304 | 44.2% |
| 2–3 s | 0.023 | 4.8% | 0.030 | 5.5% |
| 3–5 s | 0.018 | 4.3% | 0.018 | 4.3% |

Pooled over the 302 frames, where drawn: mean 0.026, 99th percentile 0.085. Over all drawn: mean 0.180, 99th percentile 0.562. A run takes 4.4 s on 14 threads (two renders per frame: the frame, and its coverage without the background).

## What the numbers show

- **Motion in the first two seconds.** Over all drawn, most pixels fail until 2 s: the reference's elements arrive much sooner. Every move in the project runs through an `outExpo` expression from a common easing script, which the import reports and leaves out, playing the linear keys under it. This is the largest gap and the reason expressions come next.
- **The settled layout.** From 3 s on, 4.3% of the drawn pixels fail. The error images put them on two things: a one-pixel fringe along the diagonal edges of the squares (anti-aliasing against the reference's chroma subsampling and compression), and a fringe on one side of every glyph of the title, which sits a few pixels off. The text geometry check below finds the cause.
- **The hashtag reveal.** The hashtag shows from its first frame, where the reference reveals it through an inverted alpha matte; mattes are not supported yet.

## Text geometry

`motion import` now holds every imported text layer, laid out by the scene, against the lines the project stored for it: each line's start, baseline, width and character count, within 0.25 px.

| Text | Font | Style | Off by | Our width / stored |
|---|---|---|---|---|
| `#mixkit` | Lato Bold 44 | | 0.00 px | 147.57 / 147.57 |
| `Learn More` | Lato Bold 44 | | 0.00 px | 224.90 / 224.90 |
| `Keep` / `your life` / `simple` | Lato Black 55 | faux bold, all caps | 7.48 px | 135.85 / 141.26, 276.24 / 291.19, 194.07 / 204.65 |

Two texts match the stored layout exactly, kerning included; the title is 4 to 5.5% narrower on every line, so each centred line starts 2.7 to 7.5 px too far right. The reference agrees with the stored layout, not with ours: measured on frame 150, the title's ink is 138, 290 and 202 px wide there against our 133, 277 and 193. Its stems are also about 1 px thicker (10 against 9 px on the `I`).

What makes the difference is faux bold. The glyph ids and the font version (Lato 2.015) are the same on both sides, the style has no tracking or scaling, and all caps alone does not widen a line: a second sample sets Lato Black in all caps without faux bold, and its 80 px line matches within 0.02 px (its 300 and 722 px lines are 0.5 to 0.6% narrow, a smaller gap still to explain). The scene carries `faux-bold`, but the typesetter and the renderer ignore it: no glyph is emboldened and no advance grows. In the reference, faux bold adds 1.35 to 1.9 px to each glyph's advance at 55 px (2.5 to 3.4% of the em) and about 1 px to each stem. One sample does not pin down the rule, so faux bold is the next fix, measured by this check.

## Reading a reference correctly

The reference is H.264 in limited range with no colour tags. Decoded with the default BT.601 matrix, its yellow reads (249, 237, 9) against the project's (255, 225, 0), and 85% of the drawn pixels failed on colour alone. With BT.709, the matrix of HD video, it reads (252, 224, 0), within 3 of 255:

```text
ffmpeg -i reference.mp4 -vf "scale=in_color_matrix=bt709:in_range=tv:out_range=pc,format=rgb24" frames/f%04d.png
```

A diff of an untagged HD reference therefore decodes it as BT.709, limited range; a wrong matrix shows as a uniform colour error over every flat area.
