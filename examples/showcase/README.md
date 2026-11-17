# Showcase

Longer scenes written from scratch, dense and in several acts, to show what a professional sequence made with motion looks like. Each uses only the built-in `Inter Variable`, no media files, no brands and no real names, and a test in `crates/motion-cli/tests/showcase.rs` keeps it passing `motion check` with nothing found.

## Brand identity system

![Brand identity system](../../docs/media/showcase/brand-system.gif)

[`brand-system.json`](brand-system.json), 1920x1080, 30 fps, 32 s. The identity of a fictional studio, Halvane, built in front of you: a construction sheet draws itself, the mark is made from its geometry, the wordmark sets, the palette, pattern and applications follow, and it ends on the lockup it started from. The GIF shows 13.0–19.2 s, drawn without the film's grain to stay small.

```sh
motion render examples/showcase/brand-system.json --out brand-system.mp4
```

| Act | Label | What happens |
|---|---|---|
| 1. Construction | `construct` 0 s | Crop marks, a ruler and guides through the golden divisions draw from their middles; five rings of radius 320/φⁿ sweep on; the golden spiral draws through them; crosshairs, dimension lines and a spec column tick in. |
| 2. Mark | `mark` 3.9 s | The tile's outline and the dot's ring draw; the tile fills as the guides fade; the bite grows out of the dot (`expand` from −75 to 122, φ² of the dot) and is cut from the tile (`merge` `subtract`), with ripples; the corners round (`round-corners` 0 → 56); a light sweep crosses it. |
| 3. Wordmark | `word` 8.7 s | The mark moves into the lockup under motion blur. Type metrics draw; the letters rise out of a baseline slot one by one, skewed and blurred, while the tracking closes from +260 to −12 over two seconds; an ember wave runs through the letters and an echo ripples from the dot. |
| 4. Colour | `palette` 13.2 s | The palette opens through the mark's own outline (two masks whose paths scale ×24 about a point in the tile) while the identity pushes in. Five swatches grow; names rise, hex codes settle in random order, RGB values tick in, and two digit wheels per swatch roll to its share; a stacked bar grows, a ring draws round the accent, a wave lifts the swatches. |
| 5. Pattern | `pattern` 17.9 s | Pushed in with motion blur. One shape layer: the mark, a repeater that turns every other copy (0 → 90°), a second that turns the pair (0 → 180°) to make a 2x2 pinwheel, and two that tile it, with a scale ramp and a slow diagonal offset. |
| 6. Applications | `apps` 22.7 s | The pattern shrinks into a sheet-framed window and becomes a display; the camera pulls back past a phone and two business cards to a wayfinding sign in one move, over a drafting-grid floor, with a pane of registration crosses close to the lens. |
| 7. Lockup | `lockup` 28.7 s | The sign's dot grows to fill the frame and closes into the lockup's dot; the tile, wordmark and tagline arrive, construction rings redraw round the dot, crop marks return, and a sweep crosses the word. Complete at 30.7 s, holding 1.3 s while the rings and sweep finish. |

**The motion system.** Every move uses one of five curves, and durations grow with distance (12 frames plus one per 40 px, up to 30):

| Curve | Used for |
|---|---|
| `cubic(.16,1,.3,1)` | entrances: fast off the mark, long settle |
| `cubic(.7,0,.84,0)` | exits |
| `cubic(.65,0,.35,1)` | moves from rest to rest, relayouts, bites, turns |
| `cubic(.45,0,.2,1)` | line draws |
| `in-out-sine` | the camera and drifts, one key pair each |
| `out-back(1)` | landings only, 3.7 % overshoot |

Staggers are 2 frames (67 ms). A move leads and its fade follows: opacity has its own animator or track, on `out-sine`, never linear on a moving thing.

**What it uses.** Text animators by character and word with `range` selectors in fraction and `index` units, `ease-high`/`ease-low` on a `triangle` wave, `tracking`, `blur`, `skew`, random `order` with `seed`; trims with offsets, `merge` `subtract`, `round-corners`, `expand` with `copies` and `copy-offset`, four nested repeaters; masks with keyed paths and track mattes; ten compositions with slots and `overrides` (the digit wheel is one composition used ten times), pivots and parenting; a 2.5D camera with `target`, `target-z` and `orient`, 3D planes and shadow planes; `remap` (the display keeps playing the pattern, slowing to a stop); `fractal-noise` grain under `overlay`, `glow`, `gaussian-blur`, `screen` sweeps; motion blur on pivots, pushes and compositions; `in`/`out` presets and labels.

**Numbers.** 152 layers in 10 compositions, 649 keyframes. On average 8.1 animated values are changing in any frame, and no second has fewer than 2.8. `motion check` prints `OK 488`. Sampled every frame with `motion at`, the mark's move, the palette's push, the pattern's push and scale, and the camera have no frame-to-frame speed change above 25 % of their peak. The zoom into the palette, evaluated from its curve, accelerates smoothly into its cut. Of the 58 joins in the scene's curved paths, none turns by more than 10°. Rendering takes 95–210 s on an M-series Mac, depending on load.
