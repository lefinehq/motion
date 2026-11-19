Status: Draft · 2026-11-13

# Edits and checks

An agent changes a scene with one-line commands, and checks it by asking nakkash questions that come back as numbers. Neither step needs the agent to rewrite the scene or look at a single frame. This is where most of nakkash's token savings come from.

## Edits

### Examples

```
card3 fill=red delay+=10f
title.in.dur=18f
card anim.opacity="0 -> 1 @0-20f"
card anim={"scale":[[0,1],["1s",1.2,"ease-out"]]}
cards.* shadow=none
add after title {"id":"sub","text":"Now in beta","font":"Inter Variable 40 400","in":"fade 8f"}
rm accent
mv bloom top
```

### How an edit is written

An edit names a **target**, then one or more **changes**.

Targets can be:

- **a layer's address:** `title`, or `card/title` for the layer `title` inside the composition `card` draws ([D43](decisions.md#d43-one-layer-address-and-edits-per-instance))
- **the scene itself:** `scene`
- **one copy in a repeat:** `cards.3`, or its alias `card3`
- **every copy in a repeat:** `cards.*`
- **a property path:** `title.in.dur`

A change uses one of four operators:

- `=` sets a value
- `+=` adds to it
- `-=` subtracts from it
- `*=` multiplies it

There are also four structural commands: `add`, `rm` (remove), `mv` (move) and `dup` (duplicate).

### Nested paths and JSON values

A key can be a dotted path, such as `anim.opacity`, and a list element is chosen by its `id` or index in brackets: `fx[glow].radius`, `scene comps.card.layers[title].text=Hi`. The edit sets that one leaf and creates the objects on the way, so `card anim.opacity="0 -> 1 @0-20f"` adds an `anim` object if the layer has none. A value that starts with `[` or `{` is read as JSON, up to its balanced end, and may contain spaces. Any other value is a number, `true`, `false`, `none` or a string; quote a string that would otherwise look like one of those. A quoted value may contain spaces and four escapes: `\"` for a quote, `\\` for a backslash, `\n` for a new line and `\t` for a tab, so `title text="Make\nit move"` sets two lines.

`none` removes the key. If that leaves its parent object empty, the parent is removed too, up the path, so `card anim.opacity=none` on a layer whose only animation is opacity also removes `anim`.

### Editing one instance of a composition

`card/title text="Sale"` changes the title of the composition only where `card` draws it. The change is written into `overrides` on the scene's layer the address starts at, so `card` gets `"overrides":{"title":{"text":"Sale"}}`, and every other layer drawing the same composition is unchanged. A deeper address writes `overrides["label/title"]` on its first layer. An override replaces whole keys, so a change inside a key, such as `card/title anim.x=…`, starts from the key as that instance draws it and keeps its other tracks.

`none` on an instance takes back the instance's own value. A key the composition itself gives the layer cannot be removed through one instance; the error says so and shows the edit of the composition, which changes every instance: `scene comps.label.layers[title].fill=none`.

### Editing a slot

A key written `$id` edits a slot ([D44](decisions.md#d44-slots-are-named-values-that-layers-follow)). On the scene it sets the value the scene's slot declares, and a line that starts with one needs no target: `$accent=#2A9D8F` is `scene $accent=#2A9D8F`. The slot keeps the form it is written in, its label too; a value of another type is written out, `{"number":"Sale"}`, so the check names the slot's type. On a `comp` layer, `card-2 $title=Sale` gives that instance its own value, written as `overrides["$title"]`, and `card-2 $title=none` takes it back. `$accent=none` removes a slot only when no value follows it; otherwise the error lists the values that do. A composition's own slots are keys like any other: `scene comps.card.slots.title.text=Hi`.

`nakkash slots scene.json` lists every slot of the scene and of each composition, with its type, value, label and role, and how many values follow it. `--json` prints the same as data, with the place of each value that follows, such as `plate.fill`, and for a text slot its font, size and `fit`, for a file slot its box and `fit` ([D77](decisions.md#d77-every-imported-template-names-what-a-designer-would-vary)).

- `--coverage` counts what the scene draws that no slot holds: text layers, distinct colours that can show (a colour is held when every use of it follows a slot), and image and video layers, and lists where each loose one is.
- `--verify` changes each slot of the scene alone, a composition's own slots too (longer words, the colour's hue turned half round or a neutral made a colour, a number moved or a share halved, a test card for an image, `--clip` for a video), draws it at half size where what follows it shows, at up to eight frames chosen from 96 by how many of its layers show there, at any depth, and reports the pixels that moved; a `dur` slot is checked by the length it gives. It then changes every slot at once and checks that the scene still draws and its check finds no more errors. It exits 1 when a slot changes nothing, does not apply, or the scene breaks with all of them. With `--mark` it writes what it saw into the scene: `"seen": false` on each slot that changed nothing, so variants skip it, and no mark on each it saw change; `--coverage` counts the slots marked so. A slot is most often unseen because what follows it is drawn under something or not drawn yet, so the count falls as the renderer closes those gaps.

### Editing a role

A target written `@role` is what the scene's role names ([D50](decisions.md#d50-roles-name-what-a-masters-layers-do)): `@headline text="Sale"` is `title text="Sale"` when `headline` is `title`, and `@accent=#2A9D8F` is `$accent=#2A9D8F` when `accent` is the slot `$accent`. A brief written against roles runs on any master that has them. A layer's role takes keys and a slot's role takes a value; the error says which. `nakkash roles scene.json` lists each role with what it holds, and `--json` gives the same as data.

### Locks

An edit that touches what the scene's `locks` hold is refused, and the scene is left as it was ([D51](decisions.md#d51-locks-are-refused-in-the-edit)). A locked layer cannot be edited, nor anything inside it, nor reached from outside: an override into it, the composition one of the layers on the way to it draws, or the scene's `layers`. A locked slot cannot be set. With `palette` locked, an edit may only set colours the scene already has, compared as 8-bit colours; with `typefaces` locked, only font families it already uses. The message names the lock, `` `logo` is locked by `@logo` ``. `locks` and `roles` are scene keys like any other, which the designer edits; a batch cannot.

### Batches of variants

`nakkash batch variants.json` makes every variant of one master ([D52](decisions.md#d52-a-batch-makes-every-variant-from-one-master)). The file names the master and an output folder, relative to itself, and lists the variants, each an `id`, its `edits` and optional `render` settings (`codec`, `quality`, `frames`, as `nakkash render` takes them):

```json
{"master":"launch.json","out":"out","variants":[
  {"id":"de-sale","edits":["@headline text=\"Sommerschluss\"","@accent=#2A9D8F"]},
  {"id":"fr-sale","edits":["@headline text=\"Fin de l'été\""],"render":{"codec":"hevc"}}]}
```

Each variant starts from the master. Its edits are applied in order, through the locks, then the scene is checked. A refused edit or a check error is reported and the variant is not rendered; the others render, one at a time, to `out/<id>.mp4` (`.mov` for a codec that keeps alpha), with the variant's scene beside it as `out/<id>.json`. A variant cannot edit `roles` or `locks`. The report is one line per variant and a count; `--json` gives each variant's outcome as data, and `--check` applies and checks without writing anything. The command exits 1 when any variant is refused or fails its check.

```text
de-sale   2 edits  check OK  out/de-sale.mp4  2.1 s
fr-sale    1 edit  refused: `#00FF00` is not one of the scene's colours, which are locked by `palette` (edit 1)
1 rendered, 1 refused, 0 failed their check
```

### Editing one copy in a repeat

`card3 fill=red` does not split the five cards into five separate layers. It stores a small override on card 3, and the repeat stays one compact layer.

### What comes back

Every edit returns two things:

1. **The new values** of everything it touched.
2. **What changed in the checks:** which problems it fixed and which it introduced.

The agent sees at once whether the edit helped.

### Cost

We measured the edit "make card 3 red and bring it in 10 frames later" in three ways:

| How the edit is made | Tokens |
|---|---|
| Rewriting the whole scene | 203–2,053 |
| JSON Patch | 29 |
| A nakkash edit | 9 |

All three figures are measured.

JSON Patch (RFC 6902) is still accepted, as a fallback for tools that prefer it.

## Checks

The goal of checks is that an agent never has to watch the video to find out whether it works.

### Analytic checks

Analytic checks are computed directly from the scene, for every frame. Nothing is rendered.

| Command | What it answers |
|---|---|
| `check` | Runs the standard rules over the whole clip and reports only the problems |
| `at 2s title cards` | Where these elements are at 2 s, and how visible they are |
| `assert before(title.in, cards.in)` | Whether a statement holds, and if not, on which frames it fails |
| `timeline` | One line per element: when it enters, settles and leaves |
| `nakkash card3` | How fast it moves at its peak, whether its motion is smooth, when it comes to rest |
| `diff` | What changed in the checks since the last edit |
| `explain cards.3` | The final, fully worked-out values, so the agent never does stagger maths itself |

`assert` understands three kinds of statement:

- **Time:** `before`, `during`, `overlaps`.
- **Position:** `above`, `left-of`, `aligned`, `inside`.
- **Measurement:** `gap(cards)=40±2`, `dir(card3.in)=up`, `visible(card3, 1s..5s)`.

`at` prints `id x=… y=… w=… h=…` for each element, then only the other properties that differ from where they rest by default, in registry order, such as `rotate=-8`, `scale=0.8,0.8` or `opacity=0.68`. An element at rest shows nothing extra, so the line stays short.

### What `check` looks for

- **Placement:**
  - elements off-screen or outside the safe area
  - elements hidden behind others by accident
  - edges that are almost aligned (2–5 px off)
- **Text:**
  - text that overflows or is cut off
  - text on screen too briefly to read for its length
- **Visibility:**
  - elements that never become visible
  - elements that end up hidden by mistake
- **Motion:**
  - sudden jumps or jitter
  - a motion cut off by the end of the clip
  - an uneven stagger
- **Pacing:**
  - stretches where nothing happens
  - too many things moving at once
- **Colour:** text without enough contrast against what is behind it.
- **Assets:** missing fonts or files.

**Built so far:** `offscreen` and `pin` (errors), `unsafe`, the craft rules of [D59](decisions.md#d59-craft-lint-measures-motion-from-the-scene-and-reports-numbers) that read the scene, `judder`, `text_hold`, `easing`, `crowd` and `overlap` (warnings), and its pixel gates, `contrast` and `flash` (errors), which `nakkash check` runs after them. Each finding is one line: severity, rule, layer, frames, then what was measured. A layout rule prints how far the layer leaves its area, `+12px`. A craft rule prints the worst value with its unit, then the limit it misses, `24px/f >11`, `0.4s <1.3`, `6 >4` or `2.1:1 <4.5`. A fix follows when one edit clears it, and the last line counts the rules that ran and found nothing:

```
E offscreen card 0-12f +40px; fix: card at="center 980"
W judder badge 1-10f 150px/f >11; fix: badge mblur=true
W text_hold title 0-9f 0.3s <0.6
W easing card 0-20f 1 <1.4
W crowd scene 4-18f 5 >4
W overlap title 0-29f +6px over sub
OK 12
```

- **`pin`:** a `corner-pin` whose corners make no quad at a frame where its layer shows: crossed (two sides meet), bent (a corner inside the other three), flat (three corners on a line, or two at one point) or unplaced (the layer has no inverse, or a pivot lies past its horizon). The layer draws nothing there ([D89](decisions.md#d89-a-corner-pin-lays-its-layers-plane-through-a-homography)). The line names the first fault of each run of frames, `E pin app 11-20f crossed: top-right–bottom-right meets bottom-left–top-left`; there is no fix, since which corner is wrong is a choice. It runs only on layers with a corner pin.
- **`judder`:** a layer that moves on its own steps farther in one frame than 11 px on a 1080-px-tall canvas (scaled by the canvas's height), for 1/8 s or more: the peak of an ease passes. Its own box's corners are measured on the canvas, and a layer moved only by its parent or composition is left to that one. A layer with `mblur` under an open shutter is smooth, so it is skipped, and turning `mblur` on is the fix.
- **`text_hold`:** a text layer stays readable (shown, at least 90% opaque, moving no faster than the judder limit) for less than its characters at 20 a second, and never less than 0.5 s. Its longest readable stretch is judged: a text read long enough once has been read, so the slow start of a push it rides does not count against it.
- **`easing`:** a move at one speed. A move is a run of at least 6 frames in which a layer's own transform carries the centre of its box; its peak speed over its mean speed is at least 1.4 when it eases (constant speed is 1). A drift slower than 2 px a frame is left alone. No fix is printed yet, since an edit cannot set a key's curve.
- **`crowd`:** more than 4 layers moving on their own at once, each faster than 2 px a frame. It is the scene's, printed on `scene`, and counted only once something moves. No fix: which layers to hold back is a choice.
- **`overlap`:** two text layers read at once whose glyphs run into each other, such as one line's descenders on the next line's capitals. Each glyph is the box its outline covers on the canvas, after its animators; two glyphs overlapping by more than 1 px on a 1080-px-tall canvas on both axes run in, and the deepest pair is printed. Read means as `text_hold` reads it, and a glyph its animators leave under 90% opaque is not read. It is printed on the layer drawn on top, with the other after it: `W overlap title 0-29f +6px over sub`. No fix: which one moves is a choice.
- **`contrast`:** a readable text layer's glyphs against what is drawn behind them, by WCAG 2's contrast ratio: at least 4.5:1, or 3:1 for text of 24 px and up on a 1080-px-tall canvas. Twice a second, the scene is drawn at a quarter of its size without the layer and with only the layer; the ratio 90% of the glyphs' coverage reaches is measured. No fix: which colour to change is a choice.
- **`flash`:** a quarter or more of the frame flashing more than 3 times in a second. Every frame is drawn at a quarter of its size, in 4 × 4 cells; a cell changes when its mean relative luminance moves at least 0.1 from its last turn, with the darker side below 0.8, and a flash is a change and its return. It is the scene's, printed on `scene`.

**A baseline.** A template may fail its own check before anyone edits it, so its errors would fail every edit. `nakkash check scene.json --baseline template.json` checks both and prints only what the baseline does not already have, after a line `KNOWN 3` counting what it left out; the exit code counts what is printed. A finding is known when the baseline has one by the same rule on the same layer measured as bad or worse, whatever its frames: an edit that moves a problem in time does not make it new, one that pushes a layer farther out does. `nakkash edit` reports the same way against the scene before the edit, so it exits 1 only when the edit made a new error.

`Session::check` reads the scene and takes milliseconds, so edits print it; `Session::gates` draws every frame, a fraction of a render's time (a 40 s 1080p scene gates in about a second on 10 threads).

Placement checks use the **transformed bounding box**: the layer's size times `scale`, turned by `rotate`, at every frame. A card that is on screen at rest but swings out while it rotates, for longer than half a second, is reported.

They measure only the part that draws, and a pivot or a sound, which draws nothing, may sit anywhere. A layer showing a composition is not measured by its box, the composition's whole canvas, which may reach past what it draws: its own layers are measured where they land, at any depth, so a composition placed, scaled or cropped whole is judged by what it draws. A composition clips what it holds to its canvas, and an alpha or luma matte clips the layer to the box its matte layer draws, so a title rising inside a matte slot is measured inside the slot; an inverted matte limits nothing. A layer wholly off the canvas is not checked while it waits there, nor while it passes through the edge on a move in from there or out to there, as a push or slide does. Nor is one that moves across the edge for half a second or less and then leaves it, back inside or out of sight, as a zoom through the frame or a glitch's copies do: half a second is the least time any text is held to be read, so nothing crossing for less is read cut off. A layer at rest across the edge is reported from the frame it stops, as is one that moves across it for longer, and one that never comes onto the canvas is reported at every frame it waits off it. Passing through depends on where a layer moves and for how long, never on its size, so a zoom of copies through the frame is judged alike at any scale, as in a master and its copy in another format.

### What a report looks like

```
E overflow cards.3.text 37-150f +18px; fix: cards.3 font.size<=34
W contrast accent/bg 2.9<4.5 0-40f
OK 41
```

Each line gives:

- **the severity:** `E` is an error, `W` is a warning
- **the rule** that failed
- **the elements** involved
- **the frames** where it fails
- **the measured value against the limit**
- **a fix**, written as an edit the agent can apply directly

A fix moves `at`, which sets the resting position. It skips an axis that an `anim` track drives, because moving `at` would not change that axis; if no axis is left, the line has no fix.

A clean scene reports one short line, for example `OK 42`.

The fix suggestion matters. In published research on checking AI-made animations (MoVer), giving the model a detailed report instead of pass/fail cut the remaining failures from 22% to 6%.

### Pixel checks

Some things cannot be judged from geometry alone: how strong a glow is, how an effect turns out, or text contrast over a photo or gradient. For these, nakkash does the following:

1. It renders a few frames at thumbnail size, for example 160×90.
2. It measures them.
3. It returns only numbers: brightness and contrast per region, blank frames, flicker between frames, blown-out highlights, and the real contrast of text over its background.

The agent still never sees an image.
