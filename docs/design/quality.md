Status: Draft · 2026-11-13

# Quality

How we decide whether motion's clips are good enough, and how we prove it in public. There are five layers:

1. **Gates**, which reject what can't ship.
2. **Fidelity tests**, which catch rendering regressions.
3. **Craft lint**, which measures timing and layout from the scene.
4. **Requirement checks**, which confirm the clip did what the brief asked.
5. **Blind human votes**, which have the final say on taste.

Everything except taste is automated. Taste is judged by people only; there is no AI judge (see [D22](decisions.md#d22-no-ai-judge)). The evidence behind this plan is in the [research note](../research/2026-11-13-quality-measurement.md).

## Who judges what

**The lead motion designer** is the project's quality owner. They:

- write the presets and default easings that agents pick from, so quality is built in;
- make the reference clips we measure against;
- set and tune the craft-lint thresholds;
- sign off the side-by-side sheet at each milestone (see [D16](decisions.md#d16-quality-gates)).

**Independent raters** judge blind tests, because nobody can blind-judge their own work:

- about 50 screened crowd raters;
- 10–15 outside motion designers.

Public quality claims rest on their votes, not on ours.

## The five layers

| # | Layer | What it catches | How |
|---|---|---|---|
| 0 | **Gates** | Clips that can't ship | Automatic pass/fail: text inside the title-safe area, flashing limits, loudness, text contrast, no missing glyphs, the right format |
| 1 | **Fidelity** | Engine regressions | Automatic pixel comparison against golden images; people review the difference images |
| 2 | **Craft lint** | Timing, easing, judder, text hold, layout | Computed exactly from the scene, with no rendering; doubles as feedback to the agent |
| 3 | **Requirements** | Whether the clip does what the brief asked | Per-brief predicates, the same `assert` checks agents use |
| 4 | **Human verdict** | Taste and ranking | Blind pairwise votes, ranked with confidence intervals |

Layers 0–3 run in CI on every change. Layer 4 runs at milestones.

## Fidelity

Each test escalates only if the previous one fails:

1. **Exact match** against the golden image.
2. **Small tolerance:** a pixel may differ by up to 0.016 per channel, on at most 1% of pixels. This is the rule Blender's render tests use.
3. **Perceptual difference:** the ꟻLIP mean, plus a high percentile of its error map so that one broken letter isn't averaged away. Results above a calibrated limit go to a person.
4. **Delivered files:** SSIMULACRA2 per frame on the encoded output. 90 or above means visually lossless. VMAF and banding checks run on the final MP4 only.

Text is checked separately. The engine knows exactly where every glyph is, so text regions get stricter limits.

**Built in from the start** (see [D21](decisions.md#d21-linear-light-float-compositing)):
- linear-light, 16-bit float compositing;
- dithered output;
- true motion blur;
- sub-pixel positioning.

These are the places where browser-based rendering loses quality.

## Craft lint

Every rule is computed from the scene, at every frame. Each rule is marked with the strength of its evidence. Rules backed only by convention give warnings, never errors.

| Rule | Starting limit | Basis |
|---|---|---|
| No constant-speed motion on UI elements | peak speed ÷ average speed ≥ 1.4 (constant speed = 1.0) | Experiment: eased motion is easier to follow (Dragicevic et al. 2011) |
| Not too many independent movers at once | ≤ 4 at moderate speed, fewer when fast | Experiment: people track about 4 objects |
| Text stays long enough to read | ≥ the reading time at 20 characters per second, minimum 5/6 s | Standard: streaming subtitle guidelines |
| No flashing hazards | ≤ 3 flashes per second | Standard: WCAG 2.3.1 |
| Readable contrast | 4.5:1, or 3:1 for large text | Standard: WCAG |
| Text inside the safe area | inside the central 90% of the frame | Standard: broadcast title-safe |
| No judder on fast moves | speed below the judder limit for the frame rate, roughly 11 px per frame at 24 fps on a 1080p frame, unless motion blur is on | Industry rule of thumb |
| Durations in a sensible range | UI-scale moves 100–500 ms, longer for larger distances | Convention: design-system guidelines |
| Overshoot is deliberate | bounce under a few percent unless the preset asks for more | Convention |
| Stagger is even | equal offsets within a group | Convention. Research found staggering doesn't help viewers track objects, so this is a warning only. |

The lead motion designer tunes these limits against the reference clips. Clips we deliberately break must trip the rules; good clips must pass.

## Reference projects and the "can you tell?" test

The lead motion designer's real projects are the reference set. The exporter also saves the tool's own render, so fidelity is measured pixel by pixel against the original: there is no guessing about what "on par" means.

- **Masters:** motion's render of every imported master must match the tool's render within the [fidelity](#fidelity) limits. Frames that miss become issues: missing features or rendering bugs.
- **Variants:** raters see an AI-made variant and one the designer made by hand for the same brief, without labels, and pick the better one or the hand-made one. We claim "on par" only if raters can't pick the hand-made one more than 60% of the time. The upper end of the 95% confidence interval must stay below that limit, and the limit is fixed before the test. A full round is about 1,600 trials.

## The public scoreboard

The board compares engines on the same briefs: motion against HyperFrames and Remotion.

- **Briefs:** 30–50, published in advance and grouped by use case.
- **Votes:**
  - blind and pairwise
  - 300–600 votes per engine pair
  - ranked with a Bradley–Terry model
  - confidence intervals that account for the same rater voting many times
- **What gets shown:**
  - Entrants with fewer than about 50 comparisons are hidden.
  - A rank difference is claimed only when the confidence intervals don't overlap.
  - 16:9 and 9:16 clips are separate pools, and so are clips with and without sound.
- **Alongside quality,** we publish cost in tokens, agent steps, wall-clock time and render time.

Fairness rules. Without them, a board run by motion is just a vendor grading itself:

- The agent and model stay the same. Only the engine and its official guidance change.
- An extra round gives every engine the same design guidance, because guidance alone can decide results.
- Engine versions and briefs are registered before the run.
- Every variant tested is published, with no hidden best-of-N.
- Cross-engine checks run on rendered pixels, never on motion's JSON, which competitors don't have.
- Briefs, code, renders, agent transcripts and anonymised raw votes are released. Rendering is deterministic, so anyone can rerun the board and get the same frames.
- Competitors are invited to submit their own best guidance.

A second board falls out of the same setup: the engine stays fixed and the AI models driving it are ranked.

## When

| Milestone | Quality work |
|---|---|
| M1 | Gates and craft lint at scene level; a planted-defect set |
| M2 | The fidelity suite; the first human calibration round, a few hundred votes |
| M3 | The reference clips and the "can you tell?" test |
| M4 | The public scoreboard |
