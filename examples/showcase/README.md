# Showcase sequences

Long, dense films written from scratch, each in several acts, with something meaningful moving at every moment. They use the built-in `Inter Variable` and no media files, invent their products and copy, and pass `motion check` with nothing found; a test in `crates/motion-cli/tests/showcase.rs` checks them. The [examples](../README.md) show one idea each; these show how the ideas combine.

## Product launch

![Product launch](../../docs/media/showcase/product-launch.gif)

[`product-launch.json`](product-launch.json), 1920x1080, 30 fps, 39 s. A launch film for Arclet, an invented task app, in eight acts. The beats are root `labels`; each act is a composition drawn by one `comp` layer.

| Act | Time | What happens |
|---|---|---|
| Hook | 0–3.7 s | "Everything is urgent." is readable at frame 0 while nine columns of notifications scroll past at their own depths, far ones blurred; "So nothing is." answers at 1.1 s. |
| Problem | 3.1–9.1 s | Pushed in from below. Five notifications drop onto a stack, each pushing the older ones down as one body; an unread badge rolls its count and runs to "9+". The stack folds into one card, which becomes the app's tile. |
| Reveal | 9.1–13.3 s | The tile grows into a phone's screen as the phone turns in, a light sweeps the glass, the splash wipes up and the list arrives. "Meet Arclet." |
| Priorities | 13.3–18.3 s | A chip turns "Urgent"; its row lifts, casts a deeper shadow and rises to the top while the rows it passes dim and make room. A task is checked off, struck through and sinks to the bottom; the progress bar and its count follow. |
| Focus | 18.3–24.7 s | A circular wipe from the checked box opens the focus screen as the phone swings across. Three toggles flip in turn; three alerts arrive and fly into a digest card, each leaving a dot. |
| Insight | 24.7–31.7 s | The week screen pushes in. The camera pulls back while the chart card lifts out of the phone and lands full size: the same composition at the same moment, so the hand-off is exact. Bars and gridlines grow, a trend line draws, the best day turns brand colour and gets a label. |
| Statement | 31.7–35.4 s | The best day's bar grows until it is the frame. "Less noise." assembles from scattered, turned characters; "More done." lands; rings drift behind. |
| End card | 35.4–39 s | The field closes into the tile, the mark draws on, the dot lands in a notch cut from the tile, ticks burst once, and the name, tagline and call to action settle. Complete at 37.6 s and held to the end. |

### The motion system

Every move uses one of five curves, each starting from rest so no move jumps from standing to full speed between frames:

| Curve | Used for |
|---|---|
| `cubic(.4,0,.2,1)`, 20–34 frames, longer for longer moves | entrances |
| `cubic(.65,0,.35,1)` | moves from rest to rest: re-sorts, pushes, wipes, data changes |
| `cubic(.55,0,.8,.25)` | exits: slow start, gone at speed |
| `cubic(.45,0,.55,1)` | camera, rig and long moves, one key pair each |
| `cubic(.4,0,.25,1.3)` | emphasis where something lands: 4.5 % overshoot |

Opacity follows the move it belongs to (`out-quad` in, `in-quad` out, starting a few frames after the move), and is never linear on a moving layer. Parts of a text take turns 1 to 3 frames apart (33–100 ms); grouped elements follow their leader 2 frames apart.

### What it uses

- **Text animators:** per character, word and line, with `random`, `center` and `last` orders; blur, skew, rotation, tracking and scale that clear as each part lands; a slow tracking drift over a hold; exits that are animators of their own.
- **Shapes:** repeaters that scroll the feeds (the layer stands still, its copies move) and lay out a plane of dots with two nested repeaters; `trim` on gridlines, a check mark, a trend line, a ring, an outline and the logo; `merge` `subtract` cutting the notch for the dot; `round-corners` rounding the closing field into the tile; `expand` rings and a `repeater` burst of ticks.
- **Masks and mattes:** a circular mask, its vertex count fixed, that wipes the focus screen open; a rounded mask on the phone's screen; light sweeps through a hidden screen-shaped matte.
- **Compositions:** 16, nested up to four deep. Rows, toggles and alerts are `collapse`d instances with `slots`; the app is drawn at twice its size for the pull-back; a notification stack is a parent chain, so one arrival moves every older card as one body.
- **2.5D:** a camera that drifts and pulls back with `target`; a 3D phone rig turned by a 3D pivot; feeds, a halo and a dot plane at their own depths, blurred by distance.
- **Effects and blending:** `four-color`, `ramp`, `gaussian-blur` for depth of field, `drop-shadow`, `glow`, `add` and `screen` blends, motion blur with a per-composition shutter.

### Numbers

| Measure | Value |
|---|---|
| `motion check` | `OK 742`, nothing found |
| Compositions, layers | 16 compositions, 170 layers written, 256 drawn once compositions are expanded |
| Animation | 284 tracks, 739 keyframes, 126 text animators, 418 shape items and effects |
| Elements in motion | 9.5 per frame on average (7.5 without the two background glows), up to 29; no frame where nothing moves; the longest stretch with only the glows moving is the 16-frame hold at the end |
| Smoothness | 25 main movers sampled every frame with `motion at`: no change of velocity between two frames above 25 % of the mover's peak speed |
| Paths | no turn above 10° between two frames while a mover goes faster than 10 % of its peak speed |
| Render | 289 s for 1172 frames on a 12-core laptop (about 4 frames a second), H.264 |

An element is a layer, shape item, effect, mask or text animator with a track, counted while that track moves and its layer shows.

The test runs `check`'s rules over every frame, in 6 s in a debug build. The pixel gates (`contrast`, `flash`) draw the film, which takes about 16 minutes in a debug build, so after an edit run `motion check` from a release build, where the whole check takes about 10 s.

To render it:

```sh
motion render examples/showcase/product-launch.json --out product-launch.mp4
```
