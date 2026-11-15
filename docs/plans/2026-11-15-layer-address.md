Status: Draft · 2026-11-15

# One layer address, and edits per instance

**Goal:** [D43](../design/decisions.md#d43-one-layer-address-and-edits-per-instance). An agent edits any text, colour or value of an imported template, including inside nested comps, with one line. It also reads every place the tools report in the same form it edits with:

```text
$ motion check launch.json
E offscreen card/title 0-9f +40px; fix: card/title at="330 100"
$ motion edit launch.json 'card/title text="Try it"'
```

Today the same scene reports nothing for `card/title`, because `check` never looks inside a comp, and the edit fails because `card/title` is not a target.

**Done means:**
- One `Address` type in motion-format, with a `winnow` grammar, a printer, and a resolver over the document, the `Scene` and the `SceneGraph`.
- Every place the tools report is an `Address`: diagnostics, `check` violations and their fixes, `at` lines, the import report, the fidelity `ignore`s and the export's sources.
- `motion edit` reaches a layer at any depth. An edit through a comp layer is an override on that instance.
- `check` and `at` see every layer at any depth, in scene space.
- The audit's workstreams 1 and 2 are closed (see the [audit](../research/2026-11-15-architecture-audit.md)).

## Design

### The address

```text
address := "scene" keys? | layer ("/" layer)* keys?
layer   := id
keys    := ("." key ("[" (id | index) "]")?)+
```

- **Built from the id rule.** The address is built on an `Id` newtype that holds the id rule, `[a-z][a-z0-9_-]*` minus the reserved `scene`. Its `winnow` piece goes in `motion_format::grammar`. The hand-written `is_valid_id` loop goes, and the behaviour shorthand stops importing it from `validate`.
- **Typed, with one printer.** `Address { layers: Vec<Id>, keys: Vec<Step> }`, where `Step` is `Key`, `Id` or `Index`, the same shape as `sources::Step` from PR #38. That type moves here and becomes this one. `Display` is the only printer, and `FromStr` reads it back.
- **One resolver.** It maps an address onto the document's JSON for edits, onto the `Scene` for validation, and onto the `SceneGraph` and each frame's state for `at` and `check`.

### Diagnostics

`Diagnostic.path` becomes an `Address`, replacing its `String`. `under` puts an outer step in front, as today. The printed form changes from `comps.card.layers[1].rect` to the address, `card/title.rect` when the error is under a layer. An error in the comp's own definition still reads `scene comps.card…`. The error-message snapshots of each crate change once.

### Overrides

A comp layer may carry `overrides`: an object keyed by an address relative to the comp it shows. Each value is an object of layer keys, which are set over that inner layer's own keys.

```json
{"id": "card", "comp": "label", "overrides": {"title": {"text": "Sale"}, "badge/icon": {"fill": "#E63946"}}}
```

- **Validation.** Each target must exist. The inner layer, with its overrides applied, is validated as any layer is, so an error points at `card/title.text`.
- **Compile.** A comp layer with overrides gets a comp graph of its own: the comp with its overrides applied. Comps without overrides stay shared, as today.
- **Edits.** `card/title text="Sale"` writes `overrides.title.text` on `card`. The same edit at three levels, `a/b/c`, writes `overrides["b/c"]` on `a`. An edit of the comp itself is the key path `scene comps.label.layers[title].text=…`.
- **Export (D45).** An override on a comp that only one layer shows is written back into that comp. On a comp that several layers show, it is reported and not written, because the project has no per-instance values to patch.

### One walk over the comp tree

The scene becomes a root `Comp` plus its scene-only fields, with one validator and one walker (audit, workstream 2). motion-scene gains a single iterator over every node at a frame. For each node it yields:
- its address;
- its transform in scene space;
- whether it is visible.

`check`, `at` and the text geometry walk the tree with it. `check` runs its rules on every node, in scene space. The renderer keeps its own descent, because it builds a tree back: a composition is a clipped, isolated layer, and a matte draws inside the layer it masks. Both read the same evaluated frame, so they cannot disagree on what is shown.

## Steps

Each step is one pull request, with its tests.

1. **`Id` and `Address`** in motion-format: the grammar, the printer, the never-panic and print-then-parse proptests, and the reserved `scene`. The import's `Ids` skips `scene`.
2. **Diagnostics carry an `Address`.** Each crate's error snapshot is updated once.
3. **The scene as a root comp, and one walker** in motion-scene. The scene and its graph hold a root composition, built by the same code as any other. Render reads its background from it, and its output does not change.
4. **`at` and `check` over every node,** reported by address, with fixtures that have nested comps. A node inside a composition is measured by the part its compositions' clips let it draw, at the opacity they give it. Its fix hint waits for step 6, since an `at` edit cannot reach it until then.
5. **Overrides** in the format: validation, compile, and the error snapshot.
6. **Edits by address.** Layers at any depth, array selectors in key paths, and overrides through instances. A batch of edit lines with `--out` (audit, workstream 4) follows once edits are typed.
7. **Every report by address:**
   - the import report's findings and `StoredText`;
   - `ignore` in the fidelity tools and `hiding` in the session;
   - the export's sources.
8. **Docs:**
   - `scene-format.md`: addresses and `overrides`;
   - `edits-and-checks.md`;
   - `variants.md`, rewritten for D30, D38 and D43;
   - CHANGELOG.

## Out of scope

- **Slots and live control links:** D44, which builds on overrides.
- **What D43 leaves for later:** overriding a comp layer's size, length or frame rate, and adding layers through an override.
- **Text runs that survive a swap:** workstream 7. Until then, a text override has the same limit as `text=`: the styled runs must still fit the new text.
