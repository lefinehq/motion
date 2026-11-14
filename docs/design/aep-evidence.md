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
