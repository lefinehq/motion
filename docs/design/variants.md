Status: Draft · 2026-11-13

# Variants

In v1, motion turns a finished, professionally designed project into variants. A designer makes the master. The AI makes the versions: new copy, new sizes, shorter cuts, other languages, and its own takes, all within the designer's style. Every variant is checked before it is rendered.

## The workflow

1. **The designer finishes the master** in their desktop motion-design tool.
2. **They export it.** An exporter running inside the tool writes three things:
   - a motion scene;
   - the project's assets;
   - the tool's own render of the master, for comparison.
3. **motion builds the variant surface.** This is a compact summary of the master: its roles, beats, editable values, style and locks. The designer confirms the roles once per master.
4. **An agent makes variants.** It turns a brief into edits on the variant surface, runs `check`, and renders.
5. **The designer reviews the variants.** They approve each one, or tweak it with the same one-line edits.

## Import

The exporter runs inside the desktop tool and uses the tool's own documented scripting API (see [D24](decisions.md#d24-import-through-an-exporter-inside-the-desktop-tool)). Because the tool itself reports every value, nothing has to be guessed from the binary file.

**What maps directly:**
- layers, nested comps and parenting
- keyframes with their easing
- masks, mattes and blend modes
- text, including per-character animation
- path operations
- core-tier effects
- the 2.5D camera

**Expressions:**
- Common patterns become motion modifiers: random shake becomes `jitter`, loops become `loop`, links between properties become `link`.
- Everything else is baked to keyframes.

**Unsupported features**, such as third-party plugins, are reported one line each, like `check` output. They can be pre-rendered inside the tool as image sequences, so the look survives.

**Every import ends with a coverage report:** the share of layers and properties that mapped exactly, and a list of what didn't.

A reader that opens `.aep` files without the tool installed comes later, after a legal review.

## The variant surface

The agent works on this summary instead of thousands of keyframes, which keeps tokens low:

```
master launch-30s 1920x1080 30fps 30s
roles   headline=title  logo=logo  cta=end.cta  product=hero  bg=bg
beats   hook 0-3s | intro 3-9s | features 9-24s | cta 24-30s
text    title "Launch Week" max 18ch · end.cta "Try it free" max 14ch
style   ease out-expo, in-out-quint · dur 10-18f · stagger 4f · palette #0B0D12 #3B82F6 #FFF · type Inter 700/400
locks   logo, palette, typefaces, end card
```

**Roles** say which layer is the headline, the logo, the call to action, the product and the background.
- motion infers roles from layer names, positions and content, and the designer confirms them.
- A naming convention in the project, such as prefixing a layer name with `@headline`, skips the guessing.

**Beats** are the master's sections in time. Shorter cuts drop or shorten beats by priority.

**The style profile** is the designer's motion vocabulary, read from the master itself:
- easing curves
- the range of durations
- staggers
- transitions
- palette
- type scale
- margins and grid

**Locks** are what a variant may never change. The default locks are the logo, the palette, the typefaces and the end card. The designer can add or remove locks.

## Kinds of variant

| Kind | What changes | How motion does it | What `check` guards |
|---|---|---|---|
| **Swap** | Text, images, colours within the palette, product shots | Edits on roles | Overflow, reading time, contrast |
| **Reframe** | 16:9 to 9:16, 1:1 or 4:5 | Re-lays out each role inside the new safe zones; reframes wide shots with the camera | Safe areas, overlap, cropped content |
| **Cut down** | 30 s to 15 s to 6 s | Drops or shortens beats by priority, retimes motion while keeping each easing curve's shape, and always keeps the logo and the call to action | Reading time, motion cut off, rushed beats |
| **Localise** | Language | Replaces text and fits it within limits; mirrors layout and motion direction for right-to-left scripts; picks fonts per script | Overflow, reading time, missing glyphs |
| **Reinterpret** | Hook, pacing, emphasis, order, motion | New choices made only from the master's style profile (see [D26](decisions.md#d26-reinterpretation-stays-inside-the-masters-style)) | Craft lint, plus a style check: only easings and durations from the profile |

Variants combine. For example: German, 9:16, 15 seconds, with a new hook.

## Quality

- **The master is the reference.** motion's render of the imported master is compared pixel by pixel against the tool's own render (see [Quality](quality.md)).
- **Every variant must pass** the gates, craft lint and checks before it renders.
- **The designer's approval rate** is the main measure: the share of variants approved without changes.
- **At milestones,** blind raters compare AI variants with variants a designer made by hand.

## Later

- Creating videos from scratch (its own roadmap)
- Reading `.aep` files directly, without the tool
- Writing variants back to the tool's format, so designers can finish them there
