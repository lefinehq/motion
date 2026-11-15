Status: Draft · 2026-11-13

# Edits and checks

An agent changes a scene with one-line commands, and checks it by asking motion questions that come back as numbers. Neither step needs the agent to rewrite the scene or look at a single frame. This is where most of motion's token savings come from.

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

`motion slots scene.json` lists every slot of the scene and of each composition, with its type, value and label, and how many values follow it. `--json` prints the same as data, with the place of each value that follows, such as `plate.fill`.

### Editing a role

A target written `@role` is what the scene's role names ([D50](decisions.md#d50-roles-name-what-a-masters-layers-do)): `@headline text="Sale"` is `title text="Sale"` when `headline` is `title`, and `@accent=#2A9D8F` is `$accent=#2A9D8F` when `accent` is the slot `$accent`. A brief written against roles runs on any master that has them. A layer's role takes keys and a slot's role takes a value; the error says which. `motion roles scene.json` lists each role with what it holds, and `--json` gives the same as data.

### Locks

An edit that touches what the scene's `locks` hold is refused, and the scene is left as it was ([D51](decisions.md#d51-locks-are-refused-in-the-edit)). A locked layer cannot be edited, nor anything inside it, nor reached from outside: an override into it, the composition one of the layers on the way to it draws, or the scene's `layers`. A locked slot cannot be set. With `palette` locked, an edit may only set colours the scene already has, compared as 8-bit colours; with `typefaces` locked, only font families it already uses. The message names the lock, `` `logo` is locked by `@logo` ``. `locks` and `roles` are scene keys like any other, which the designer edits; a batch cannot.

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
| A motion edit | 9 |

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
| `motion card3` | How fast it moves at its peak, whether its motion is smooth, when it comes to rest |
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

Placement checks use the **transformed bounding box**: the layer's size times `scale`, turned by `rotate`, at every frame. A card that is on screen at rest but swings out while it rotates is reported.

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

Some things cannot be judged from geometry alone: how strong a glow is, how an effect turns out, or text contrast over a photo or gradient. For these, motion does the following:

1. It renders a few frames at thumbnail size, for example 160×90.
2. It measures them.
3. It returns only numbers: brightness and contrast per region, blank frames, flicker between frames, blown-out highlights, and the real contrast of text over its background.

The agent still never sees an image.
