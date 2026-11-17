# Showcase scenes

Long, dense scenes written from scratch, each passing `motion check` with nothing found. They show what the format holds at the scale of a finished piece; the shorter scenes in [`examples/`](../README.md) are the place to learn it.

## Data story

[`data-story.json`](data-story.json), 1920x1080, 30 fps, 38.2 s. An editorial explainer about one fictional city's day of electricity, on illustrative data that the end card labels as such. Every number on screen comes from one hourly table, so the acts agree: the clock's bars sum to the running total, the twelve districts add up to the city at every hour, and the sectors add up to the city at 09:00 and 19:00.

![The clock unrolling into the line chart, and the scrub to the evening peak](../../docs/media/showcase/data-story.gif)

| Act | Label | What moves |
|---|---|---|
| 1 | `open` | `2.4×` counts up from `1.0` on odometer wheels, the sentence rises word by word with highlight sweeps, and the × flies to the clock's centre, turning into its crosshair. |
| 2 | `clock` | A 24-hour clock with noon at the top: its ring, grid rings and ticks build (trims, repeaters), a hand with a fading wake (a repeater of wedges) leads the bars round, a running total counts to 100 GWh, and callouts mark the peak and the low. |
| 3 | `curve` | The clock unrolls into the line chart, a match cut through shared geometry: its ring becomes the x axis, its bars the columns, its outline the demand line. Gridlines and labels build, the solar series draws with its legend riding the drawing head, and a cursor scrubs the day hour by hour while its readout's digits roll. The evening band then grows into the next act's panel. |
| 4 | `mix` | Stacked bars by sector at 09:00; the clock in the corner rolls to 19:00, the bars re-measure, and the rows re-sort: falling rows step into a dimmed left lane, rising rows travel a near lane, three at a time at most. A sheen crosses the new leader. |
| 5 | `grid` | Twelve district cards in 3D, seen through a camera that pulls back from the first card in one key pair. Each card shows once it is wholly inside the title-safe area, and the two exceptions dim. |
| 6 | `close` | The takeaway in kinetic type (per-character skew, blur and a tracking wave bent by `ease-high` and `ease-low`, words rising), an underline, the day's curve as a signature, and the end card. |

**Motion system.** One set of curves runs the piece: entrances `cubic(.16,1,.3,1)`, exits `cubic(.7,0,.84,0)`, moves on screen `cubic(.55,0,.45,1)`, the camera and the unroll `cubic(.45,0,.55,1)`, and `out-back(1)` (3.7 % overshoot) only where a mark lands. Durations grow with distance (12 frames at 40 px, 19 at 200, 27 at 1500), staggers are 2 frames (67 ms), and fades start after their move and never run linear.

**Counting numbers.** The format has no expressions, so every number that counts is an odometer: a composition whose canvas is the window, holding one text layer per digit with its digits one per line, rolled by a text animator's `position`. A second animator leaves the rows outside the window faint, so `check` does not read hidden digits as text running into their neighbours.

**The unroll.** Path keys move vertex by vertex, so a circle cannot bend into a line by two keys. The generator bakes the unroll, an arc whose length and turn change together, as one key a frame for the three paths that carry it.

The scene is written by a generator kept outside the repository; the JSON is its output, formatted to be read.
