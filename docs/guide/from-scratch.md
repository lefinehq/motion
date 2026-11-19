# Writing a video from scratch

How an agent turns a brief into a nakkash scene: the loop, the numbers `check` holds a scene to, the moves the format has, and practice that is still provisional. The examples in [`examples/`](../../examples/README.md) follow it; read one before writing ([D72](../design/decisions.md#d72-scenes-written-from-scratch-get-choreography-as-data-and-a-guide)). The format itself is in [Scene format](../design/scene-format.md).

## The loop

1. **Read the brief** for its size, length and what the film must say and show. Copy comes from the brief; do not invent numbers, names or claims.
2. **Write a beat sheet** before any layer: each beat's start, what it says, and what moves. Name the beats as `labels`, `"labels":{"punch":"1.2s","body":"2.8s","end":"7.6s"}`, so spans and offsets are written against them and moving a beat is one edit.
3. **Write the scene** against the labels: one composition per beat, drawn by a `comp` layer whose `span` and `offset` follow the beat.
4. **`nakkash check scene.json`** and fix every `E` line. A `W` line is a measured problem too; fix it unless the brief asks for it. A line with `fix:` gives the edit, which `nakkash edit scene.json '<line>'` applies.
5. **Look at a few frames:** `nakkash render scene.json --frame 45 --out frames` writes one PNG. Look at the opening, each beat after its move lands, and the end card. One sheet of frames shows more than many single ones.
6. **Render:** `nakkash render scene.json --out film.mp4`.

When starting from a template instead, `nakkash check scene.json --baseline template.json` prints only what your edits added, and `nakkash edit` reports the same way.

## What `check` measures

These are the limits the check holds every scene to. They are measured, and an agent can rely on them.

| Rule | Limit | What to write |
|---|---|---|
| `offscreen` (error) | a layer that draws stays inside the canvas | Keep what is read inside the frame. A layer may wait wholly off the canvas and push in or out, or cross the edge in motion for up to 0.5 s; it may not rest across the edge. |
| `unsafe` | inside the title-safe area, 5% in from each side | Keep text and logos inside it: on 1080x1920, x 54 to 1026 and y 96 to 1824. |
| `judder` | 11 px a frame on a 1080-px-tall canvas (scaled by height), for 1/8 s | A fast move needs `"mblur":true`; the fix says so. |
| `easing` | a move's peak speed at least 1.4 times its mean | Give moves a curve (`out-cubic`, `in-out-cubic`, `out-back`), not `linear`. |
| `text_hold` | readable for its characters at 20 a second, and at least 0.5 s | Hold a line of 30 characters for 1.5 s once it has landed. |
| `crowd` | at most 4 layers moving on their own at once | Stagger entrances; let a composition or a pivot carry a group. |
| `overlap` | two read texts' glyphs do not run into each other | Leave room between lines in separate layers, descenders included. |
| `contrast` (error) | 4.5:1 against what is behind, 3:1 at 24 px and up (on 1080 tall) | Light text on dark ground or the reverse; check text over pictures. |
| `flash` (error) | a quarter of the frame flashing at most 3 times a second | No strobing cuts or flicker. |
| `pin` (error) | a `corner-pin`'s corners make a quad: not crossed, bent in, flat, or out of the layer's sight | Name corner pivots in order, top left then clockwise; `"pivots":"screen"` reads `screen-tl`, `-tr`, `-br`, `-bl`. |

## Moves

A move is one key on the layer. Presets are in [Scene format](../design/scene-format.md#presets); `in` enters, and `out` leaves the same way, reaching its pose on the last frame the layer shows.

| Want | Write |
|---|---|
| Words readable at once that still arrive | `"in":"slam 10f out-expo"` (from scale 1.6) or `"in":"rise 60px 10f out-back"` (from below); both stay opaque |
| A line that fades into place | `"in":"fade-up 40px 12f out-cubic"` |
| Words or characters in turn | `"in":"fade-up word 24px 10f out-cubic stagger 3f"` |
| A beat pushed in from below and out at the top | `"in":"push-up 12f in-out-cubic"` on the beat, `"out":"push-up 12f in-out-cubic"` on the beat before, both with `"mblur":true`, the second's span starting where the first's exit starts |
| A 3D card or phone swinging in | `"3d":true,"in":"swing-in 18f out-back"` |
| A plate that wipes open from an edge | `scale-x` from 0 under `anim`, with `anchor` on that edge |
| A slow drift that keeps the frame alive | `"anim":{"scale":"1 -> 1.15 @0-9s in-out-sine"}` |
| A layer that enters before the film starts | a negative `offset`, `"offset":"-6f"`, so frame 0 shows it landed |
| A screen or label pinned onto tracked footage | `nakkash ml track scene.json phone.tracks[screen] --to screen --corners`, then on the layer to pin `"fx":[{"corner-pin":[0,0],"pivots":"screen"}]`; nudge a corner in px of the layer with `"top-left":[4,4]` |

## Pitfalls

- **A child does not take its parent's span.** Parenting moves a layer, not its time: the children of a pivot show for their own spans. Put each beat in a composition, whose `comp` layer's span hides all of it.
- **An opaque entrance shows before it starts.** `rise` and `slam` start opaque, so a layer that enters later needs a `span` that starts with its entrance.
- **Enter after the push lands.** A child whose entrance runs while its composition is pushed moves on its own at the push's speed and judders. Give it an `offset` of the push's length or more.
- **Text is placed by the centre of its box.** To place a line by where its baseline starts, set `"anchor":[0,0]` and `position`.
- **`at` and `position` both place a layer;** write one of them.
- **A preset that turns in depth needs a 3D layer:** `swing-in` on a flat one is rejected.

## Practice that is provisional

A studio wrote about forty 9:16 films with the CLI and scored their frame sheets with an AI critic. The critic is not a judge this project uses ([D22](../design/decisions.md#d22-no-ai-judge)), and its scores moved more between batches than most of the effects it measured. What it suggested is worth trying, not relying on; no rule checks it.

- **Frame 0:** words readable and fully opaque by 0.3 s, a subject on screen, and something moving. Half-faded words at frame 0 scored worse than opaque ones.
- **Motion through the opening:** motion that lasts the whole opening beat scored better than a move that settles in half a second.
- **A second beat between 0.9 and 1.6 s,** which completes the first line: its answer, its punchline, the product landing. A slogan from elsewhere scored worse.
- **Few words in the opening:** 4 to 10, one line of at most 14 characters, 120 px and up on a 1080-wide 9:16 canvas, in the top or middle third.
- **An end card complete within 0.3 s that holds at least 1 s.** One still entering when the film ends read as cut.
- **One readable element on a desktop screen.** A whole desktop screen at phone size read as unreadable; one row or message filling 40% of the frame read better.
