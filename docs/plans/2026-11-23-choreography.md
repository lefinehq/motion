Status: Draft · 2026-11-17

# Choreography for scenes written from scratch

**Goal:** an agent writing a video from a brief gets the moves a motion designer reaches for as short data, a guide that says how to time them, and examples that show both, as [D72](../design/decisions.md#d72-scenes-written-from-scratch-get-choreography-as-data-and-a-guide) says. The engine gains exits and transitions, not taste: whether a video is good stays for people to judge ([D22](../design/decisions.md#d22-no-ai-judge)).

## What real use showed

A studio wrote about forty 9:16 films with the CLI over two days, each from a brief, and kept every scene. What the scenes repeat:

- **Exits and pushes are written by hand.** Every beat that leaves, and every push from one screen to the next, is a pair of keys per layer, and the pivots that carry a push are parked off the canvas before and after. A product film built from pushes carries dozens of such keys.
- **The same entrances, rebuilt each time:** a headline that slams in from a larger scale while already opaque, a line that rises into place without fading, a phone that swings in on a 3D turn.
- **The same mistakes:** an opening that shows nothing for half a second, words half faded at the first frame, an end card that is still entering when the video ends, two lines set on each other.

The last two now have checks or fixes of their own (`overlap`, `offscreen` leaving what waits off the canvas). The first two are what this plan adds.

What the studio's notes also say, from an AI critic scoring frame sheets: a second event in the first two seconds held attention, motion that lasts the whole opening beat a fast settle, and a complete end card beat a short one. Those are practice to try, not rules to check: the critic is the judge D22 rules out, and its scores moved more between batches than the effects it measured. The guide states them as provisional, with where they come from.

## Steps

One pull request each, in this order.

### 1. Exits

`out` is an exit, the mirror of `in`: `"out":"fade-down 12f"` plays its preset so the layer reaches the preset's pose as its span ends, and stays there.

- **Where it ends:** at the end of the layer's span, or of its composition when it has none. `@6f` ends it 6 frames earlier.
- **The pose:** a start relative to the rest (`"+d"`, `"-d"`) is mirrored, and an absolute one (`opacity 0`, `scale 0.6`) is kept, so a layer leaves the way its entrance moves: `fade-up` rises into place and rises out, `pop` grows in and shrinks out.
- **Curves:** the curve applies from the rest to the pose, so `ease-in` leaves slowly and speeds away.
- **Stagger** splits as an entrance's does, and the last part ends at the end.
- **One writer per property:** `anim` may not animate what `out` animates. `in` and `out` may share a property when their times do not overlap; when they do, the file is rejected, naming both.
- **Edits:** `card out=none` removes it, as for `in`.

### 2. Moves across the canvas and new presets

A preset start may be the canvas's size: `"+h"` and `"-h"` move by the height of the composition the layer sits in, `"+w"` and `"-w"` by its width. Built-in presets added:

| Preset | Starts from | Use |
|---|---|---|
| `rise` | moved down by the distance | words that rise into place already opaque |
| `slam` | scale 1.6 | a headline that lands, opaque at once |
| `push-up` | moved down by the canvas's height | a screen pushed in from below; as `out`, pushed out at the top |
| `push-down`, `push-left`, `push-right` | the other three edges | the same from the other sides |
| `swing-in` | turned 35° about x and −20° about y, scale 0.9 | a 3D layer, such as a phone, swinging into place |

A push between two beats is then one `out` on the first and one `in` on the second, at the same moment and with the same curve, on the pivot that carries each beat.

### 3. Examples written from scratch

`examples/` gets scenes written from scratch, one per common kind: a 9:16 social film (an opening beat, a second beat, three body beats joined by pushes, an end card), a 16:9 lower third, a title card and a product film built from drawn screens. No brand, footage or real names: everything is drawn, the copy is made up.

- Each passes `motion check` with no finding; a test runs them, so an engine change that breaks one fails CI.
- Each stays short enough to read in one go, since an agent reads them as it would a guide.

### 4. A guide for agents

`docs/guide/from-scratch.md`, and the same text as an agent skill in `skills/motion/SKILL.md`:

- **The loop:** read the brief, write a beat sheet (beats, their times as `labels`, what each says and shows), write the scene against the labels, run `check`, look at a few frames with `render --frame`, fix what was found, render.
- **Numbers that are measured:** the check rules' limits (safe area, readable size, contrast, how long text holds, judder, easing, crowd), with what each means for writing.
- **Practice that is provisional:** words readable and opaque by 0.3 s with something moving; a second event between 0.9 and 1.6 s that completes the first line; motion that lasts the opening beat; an end card complete within 0.3 s that holds at least 1 s. Each says where it comes from and that it is untested by people.
- **Moves:** which preset or pattern does what, pointing at the examples.

### 5. Measure

`motion bench` on the thirty briefs, before step 1 and after step 4, with the same agent, model and prompt: steps, tokens, check errors and warnings per brief. People watch both sets side by side. The numbers say what the guide cost and what the check caught; they do not say a video is better.

## Not in this plan

- New check rules from the studio's notes. A rule needs a measure people agree with; the notes give practice to try first.
- Copywriting: what a film says is the brief's and the agent's.
- Beats as a new layer kind. `labels` already name moments, and spans and offsets are written against them.
