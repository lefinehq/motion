Status: Draft · 2026-11-13

# Edits and checks

An agent changes a scene with one-line commands, and checks it by asking motion questions that come back as numbers. Neither step needs the agent to rewrite the scene or look at a single frame. This is where most of motion's token savings come from.

## Edits

### Examples

```
card3 fill=red delay+=10f
title.in.dur=18f
cards.* shadow=none
add after title {"id":"sub","text":"Now in beta","font":"Inter 40 400","in":"fade 8f"}
rm accent
mv bloom top
```

### How an edit is written

An edit names a **target**, then one or more **changes**.

Targets can be:

- **a layer id:** `title`
- **one copy in a repeat:** `cards.3`, or its alias `card3`
- **every copy in a repeat:** `cards.*`
- **a property path:** `title.in.dur`

A change uses one of four operators:

- `=` sets a value
- `+=` adds to it
- `-=` subtracts from it
- `*=` multiplies it

There are also four structural commands: `add`, `rm` (remove), `mv` (move) and `dup` (duplicate).

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

A clean scene reports one short line, for example `OK 42`.

The fix suggestion matters. In published research on checking AI-made animations (MoVer), giving the model a detailed report instead of pass/fail cut the remaining failures from 22% to 6%.

### Pixel checks

Some things cannot be judged from geometry alone: how strong a glow is, how an effect turns out, or text contrast over a photo or gradient. For these, motion does the following:

1. It renders a few frames at thumbnail size, for example 160×90.
2. It measures them.
3. It returns only numbers: brightness and contrast per region, blank frames, flicker between frames, blown-out highlights, and the real contrast of text over its background.

The agent still never sees an image.
