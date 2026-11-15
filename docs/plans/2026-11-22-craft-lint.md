Status: Draft · 2026-11-15

# Craft lint

**Goal:** `motion check` tells an agent, in numbers, where a scene's motion falls short of the craft a motion designer applies, as [D59](../design/decisions.md#d59-craft-lint-measures-motion-from-the-scene-and-reports-numbers) says:

```
W judder card 12-18f 23px/f >11; fix: card mblur=true
W text_hold title 30-41f 0.4s <1.3
W easing badge 0-24f 1 <1.4
OK 41
```

## The rules

Each rule measures one number per layer or per frame and compares it with a limit from [Quality](../design/quality.md#craft-lint). The limits sit in one table in `motion-query`, so tuning one is a one-line change.

| Rule | Measures | Starting limit | Severity |
|---|---|---|---|
| `judder` | How far a layer's drawn box moves in one frame, on the canvas | 11 px per frame on a 1080-px-tall canvas, scaled by the canvas's height, for at least 1/8 s; skipped for a layer with `mblur` and an open shutter | warning |
| `easing` | For each move (a run of at least 6 frames in which the layer's own transform changes), its peak speed over its mean speed | at least 1.4 (constant speed is 1.0) | warning |
| `text_hold` | How long a text layer stays readable: shown, at least 90% opaque and moving no faster than the judder limit | at least the characters ÷ 20 per second, and at least 0.5 s (5/6 s before calibration) | warning |
| `crowd` | How many layers move on their own at once at more than 2 px per frame; a layer moved only by its parent or its composition does not count | at most 4 | warning |
| `contrast` | The contrast between a text layer's fill and what is drawn behind its glyphs | 4.5:1, or 3:1 for text of 24 px and up | error |
| `flash` | Changes of at least 10% in the mean relative luminance of a quarter or more of the frame | at most 3 per second | error |

## The report

A craft finding prints the measured value with its unit and the limit it misses, instead of an overshoot in px: `23px/f >11`, `0.4s <1.3`, `1 <1.4`, `6 >4`; the unit is printed once, after the value. Consecutive frames merge into one line, as the layout rules' do. A fix is printed only when one edit clears the finding:
- `judder`: turning on `mblur`;
- `easing`: none yet, since an edit cannot set a key's curve; a curve on the track that drives the move is the fix once one can;
- `text_hold`: none, since how long a span can grow depends on what follows it;
- `crowd`: none, since which layers to hold back is a choice.

## Steps

Each step is one pull request, with its tests and docs.

1. **Decision.** D59 and this plan.
2. **Measures and whole-life rules.**
   - Report lines carry a value, a unit and a limit.
   - Rules can read a layer's states over all its frames, not one frame at a time.
   - `judder` and `text_hold`.
3. **`easing` and `crowd`.** Moves are found from the layer's own transform, measured on the centre of its own box. Text animators count as one mover. A scene where nothing moves does not count `crowd` as passed.
4. **The pixel gates.**
   - `contrast` and `flash` on a render at a quarter of the size: the frame without the text layer for contrast, and the mean luminance of each frame's cells for flashing.
   - `motion check` runs them; `motion-query` stays free of rendering.
   - Contrast is sampled twice a second and measured on the ratio 90% of a layer's glyph coverage reaches. Flash counts a change of a cell of a 4 × 4 grid, from its last turn, as WCAG 2 does, with the darker side below 0.8.
5. **Calibration.**
   - A planted-defect scene per rule in the tests: each trips its rule and no other.
   - Every rule over the corpus's imported templates, measured locally. The share of templates each rule fires on is recorded in the docs.
   - Limits that fire on many professional templates are raised, or their rule is cut.
   - Done 2026-11-15: the results and the two limits that moved are in [Quality](../design/quality.md#calibration).
6. **The brief set.**
   - 30 briefs in `bench/briefs/`, grouped by use case: ads, launch and feature videos, explainers, social.
   - A runner records each brief's tokens, agent steps, wall-clock time, render time and `check` findings. It scores nothing: taste is judged by people ([D22](../design/decisions.md#d22-no-ai-judge)).
   - These are the briefs the [public scoreboard](../design/quality.md#the-public-scoreboard) will use.

## Out of scope

- Layout, grid and alignment rules.
- Colour harmony and palette rules.
- Loudness, which belongs with audio export.
- Scoring taste, by any means.
