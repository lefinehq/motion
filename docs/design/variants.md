Status: Draft · 2026-11-15

# Variants

In v1, motion turns a finished, professionally designed project into variants. A designer makes the master. The AI makes the versions: new copy, new sizes, shorter cuts, other languages, and its own takes, all within the designer's style. Every variant is checked before it is rendered.

## The workflow

1. **The designer finishes the master** in their desktop motion-design tool and saves the project.
2. **motion imports it.** `motion import` reads the `.aep` project itself, with no desktop tool installed ([D30](decisions.md#d30-read-aep-projects-directly)), and writes a motion scene with a report of what it mapped, defaulted and could not hold yet. The tool's own render of the master, or the template's preview, is kept as the reference to compare against.
3. **motion builds the variant surface.** This is a compact summary of the master: its roles, beats, editable values, style and locks. The designer confirms the roles once per master.
4. **An agent makes variants.** It turns a brief into one-line edits, runs `check`, and renders.
5. **The designer reviews the variants.** They approve each one, or tweak it with the same one-line edits.
6. **A variant can go back to the project.** `motion export` patches each edited value into the bytes the import read it from, and leaves every other byte as it was ([D45](decisions.md#d45-export-patches-the-project-it-came-from)), so the designer can finish it in their tool. A slot that came from an expression control goes back into that control, so the template opens with the variant's values.

Steps 1, 2, 4 and 6 work today on a single scene. The variant surface and batches of variants are the next milestone ([M3](roadmap.md#m3-variants)).

## Import

The import is an ordinary scene ([D32](decisions.md#d32-one-format-for-authored-and-imported-scenes)): every check, query and edit works on it as on a scene an agent wrote.

**What maps directly:**
- layers, nested compositions and parenting
- keyframes with their easing, including spatial paths
- masks, mattes and blend modes
- text, including per-character animation, with the project's own line breaks kept as evidence
- shape groups and path operations
- core-tier effects

**Expressions are baked** when the project is imported ([D38](decisions.md#d38-expressions-are-baked-at-import)): an embedded JavaScript engine evaluates each one at every frame, and its values become keys fitted to curves. A render never runs JavaScript. The report keeps each expression's source and says what it became.

**Live links come next.** Many templates drive their text and colours from a control layer, so that one value changes many. Baked, those links are frozen at the values the project saved. Keeping them live, so that a change to the control reaches every layer that follows it, is the work of [D44](decisions.md#d44-slots-are-named-values-that-layers-follow): slots, named values that layers follow, set per instance through overrides ([plan](../plans/2026-11-17-slots.md)).

**Fonts** are resolved before a render, not during one ([D39](decisions.md#d39-fonts-are-resolved-before-a-render)), and a font the import does not have is substituted and reported ([D40](decisions.md#d40-missing-fonts-are-substituted-at-import)).

**Every import ends with a report:** the share of layers and properties that mapped exactly, and a line for each thing that did not, with where it is in the project.

## Compositions and instances

A template often shows one composition several times: three cards, each a `card` composition with its own title. Every layer at any depth has one address, the ids down to it joined by `/`: `card-2/title` ([D43](decisions.md#d43-one-layer-address-and-edits-per-instance)).

- **An edit of an instance changes that instance alone.** `card-2/title text="Sale"` writes `"overrides":{"title":{"text":"Sale"}}` on `card-2`. The other cards keep the composition's own title.
- **An edit of the composition changes every instance:** `scene comps.card.layers[title].text="Sale"`.
- **`check` and `at` see every layer at any depth,** in the scene's space, and name it by its address. A fix hint for an inner layer is an edit of its instance.

So a swap that touches one card never reaches into the others, and the agent never has to copy a composition to change one use of it.

## The variant surface

The agent works on this summary instead of thousands of keyframes, which keeps tokens low:

```
master launch-30s 1920x1080 30fps 30s
roles   headline=title  logo=logo  cta=end/cta  product=hero  bg=bg
beats   hook 0-3s | intro 3-9s | features 9-24s | cta 24-30s
text    title "Launch Week" max 18ch · end/cta "Try it free" max 14ch
style   ease out-expo, in-out-quint · dur 10-18f · stagger 4f · palette #0B0D12 #3B82F6 #FFF · type Inter 700/400
locks   logo, palette, typefaces, end card
```

**Roles** say which layer is the headline, the logo, the call to action, the product and the background. A role names a layer by its address, so it can sit inside a composition.
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

- **The master is the reference.** motion's render of the imported master is compared pixel by pixel against the tool's own render (see [Quality](quality.md)). `motion diff` measures one scene against reference frames, and `motion fidelity` measures every local sample and keeps a ledger, so a change that makes any master worse shows up.
- **Every variant must pass** the gates, craft lint and checks before it renders.
- **The designer's approval rate** is the main measure: the share of variants approved without changes.
- **At milestones,** blind raters compare AI variants with variants a designer made by hand.

## Later

- Creating videos from scratch (its own roadmap)
- Templates from other motion-design tools. Some wrap a project motion already reads, with their editable values listed; others are node graphs or XML scenes, which need their own readers.
- Exporting a scene that did not come from a project
