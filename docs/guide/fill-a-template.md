# Filling a template

How an agent makes a variant of a template without reading it: read the brief, write an instance, check it, render it ([D91](../design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point)). The template stays locked; the instance holds only what differs, a few hundred tokens where the scene is tens of thousands. [`examples/instance/`](../../examples/instance/) has a template and a German instance of it.

## The loop

1. **Pin an instance:** `nakkash slots promo.json --as-instance --out de.json` writes an empty instance pinned to the template by its path and hash. Never open the template's scene file.
2. **Read the brief:** `nakkash brief de.json`. Rules and locks come first, then the sections with their story roles and times, then one table each for texts, colours, files, and choices and numbers. Write only the ids it lists; everything else is locked.
3. **Fill the slots** with one `nakkash edit` of every value: `nakkash edit de.json '$headline="Jeder Markt auf einmal"' '$accent=#0B6E4F' '$screen="shots/app-de.png"' '$ending=end-left'`. The edit writes the instance only when no value is refused, and prints every problem at once. `$id=none` returns a slot to the template's value; `$id=$hide` hides what follows it; `look=<name>` chooses a look. The instance is JSON, so writing its `slots` directly works too.
4. **`nakkash check de.json --json`** and repair every `error`. Each problem is one object with its `code`, `slot`, `measured`, `limit` and a `hint`; a `warning` renders and is listed. Then the scene's own check runs, and prints only what your values added to the template's findings.
5. **Render:** `nakkash render de.json --out de.mp4`, or `--frame 90 --out frames` to look at one frame of a section first.

## Reading the brief

- **Sections.** `role?` is inferred from the section's names, its place and what it shows; give it in the instance's `sections` when you know better: `"sections":{"product":{"role":"feature"}}`. Times are scene seconds, and labels mark the beats inside a section.
- **Text.** `≈24 ch × 2 ln @96` is about how many characters fit a line at the size the text is set at, in the template's own font, and how many lines it may take; `(≥72: ≈32)` is the floor it may shrink to and what fits there. `overflow` says what a longer text does: `shrink` down to the floor, `wrap` into more lines, `reject` nothing at all. `read` is how many characters can be read while it is on screen, at 20 a second. The numbers are a guide: `check` sets your words in the real font, and `text.overflow` says how many characters to cut.
- **Colour.** A colour is named by what it paints (`text`, `background`, `glow`, `shadow`, `gradient-1`, `highlight`, `grade-dark`), with how many values each. A colour with a pair is read against it, and `check` holds the pair to its least contrast (`color.contrast`).
- **Media.** `kind` says what a file is for: `screen` for a product's screen, `logo`, `image`, `video`, `texture`. Give a file of the `aspect` and at least the `min px` shown; `check` warns of a crop (`media.crop`) and of a file too small (`media.small`). `takes` lists the file types: a screen that takes `image|video` plays a clip given as `{"video":"…"}`.
- **Choices and numbers** list their words and their range; anything outside is an error.

## Pitfalls

- An id the brief does not list is an error, with the nearest ids as the hint; nothing is ever ignored.
- Files in an instance are relative to the instance's folder.
- When the template changes, `check` warns `template.changed`; `nakkash migrate de.json` pins it again, keeping the value of any slot the template lost under `stale`.
- `raw` sets a static value no slot holds, and never motion, timing or effects; use it only when you are asked for something no slot holds.
