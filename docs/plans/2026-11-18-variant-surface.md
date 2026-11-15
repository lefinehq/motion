Status: Draft · 2026-11-18

# The variant surface: roles, locks and batches

**Goal:** the first part of [M3](../design/roadmap.md#m3-variants). A master says which of its layers do what (its *roles*) and what a variant may never change (its *locks*). An agent then makes many variants of it in one run, each a list of edits, each checked before it renders:

```text
$ motion roles launch.json
headline  title     text "Launch Week"
cta       end/cta   text "Try it free"
logo      logo      image logo.png        locked
accent    $accent   color #E63946
$ motion batch variants.json
de-sale    12 edits  check OK   out/de-sale.mp4    2.1 s
fr-sale    12 edits  check OK   out/fr-sale.mp4    2.0 s
long-cta    3 edits  refused: `logo` is locked (edit 2)
2 rendered, 1 refused
```

Beats, the style profile and the variant kinds that need layout (reframe, cut down, localise, reinterpret) come after this part, each with its own plan. They build on roles and locks.

## What the corpus says

Measured on the 196 corpus scenes the import reads, by layer id against a fixed vocabulary (counts only):

| Role | Scenes with a layer named for it | Layers |
|---|---|---|
| logo | 43 | 127 |
| headline (`title`, `heading`) | 27 | 122 |
| background (`bg`) | 67 | 241 |
| photo (`image`, `placeholder`, `media`) | 70 | 585 |
| call to action (`cta`, `button`) | 11 | 33 |

A scene has a median of 2 text layers (90th percentile 12), and 35 have none. Names alone find a role in a minority of scenes. So roles are written in the scene, the import only proposes them, and the designer confirms once per master. A naming convention, `@headline Title`, skips the guess.

## Roles

```json
{"motion":1, "roles":{"headline":"title", "cta":"end/cta", "logo":"logo", "accent":"$accent"}, "layers":[…]}
```

- **A role names a layer by its address** ([D43](../design/decisions.md#d43-one-layer-address-and-edits-per-instance)), at any depth, or a slot by `$id` ([D44](../design/decisions.md#d44-slots-are-named-values-that-layers-follow)). Role names are ids. `check` reports a role whose address or slot does not exist.
- **An edit may target a role:** `@headline text="Sale"` is `title text="Sale"`, and `@accent=#2A9D8F` is `$accent=#2A9D8F`. An agent writes edits against roles, so the same brief works on every master.
- **`motion roles scene.json`** lists each role with what it holds, its text, its kind or its slot's value, and whether it is locked. `--json` gives the same as data.
- **The import proposes roles.** A layer named `@role Name` takes that role, and the `@role` leaves its id. Otherwise a layer whose id matches the vocabulary above takes the role when only one layer does. Each proposal is a line of the report, `role headline: title (from its name)`, so the designer can confirm or change it with an edit, `scene roles.headline=card/title`.

## Locks

```json
{"motion":1, "locks":["@logo", "end", "$accent", "palette", "typefaces"], …}
```

- **What a lock names.** A role (`@logo`), a layer address (`end`: the layer and everything inside it), a slot (`$accent`), or one of two named locks:
  - `palette`: an edit may only set colours the scene already has.
  - `typefaces`: an edit may only set font families the scene already uses.
- **Edits that touch a lock are refused**, with the lock named: `` `end/cta` is inside `end`, which is locked``. Refusing in the edit, not in a later check, means a variant never holds a locked change, and the agent learns why straight away.
- **The named locks need no stored list.** They compare an edit with the scene it edits, so a variant made of many edits can never leave the master's palette or typefaces.
- **The default locks** are `@logo`, `palette` and `typefaces`, which the import writes when it finds a logo. The end card has no role yet, so it is locked by address once confirmed.
- **Locks are the designer's.** `motion edit` can still change `locks` and `roles`, as any scene key; a batch cannot (below).

## Batches

```json
{"master":"launch.json", "out":"out",
 "variants":[
   {"id":"de-sale", "edits":["@headline text=\"Sommerschluss\"", "@cta text=\"Jetzt kaufen\""]},
   {"id":"fr-sale", "edits":["@headline text=\"Fin de l'été\""], "render":{"codec":"hevc", "quality":"master"}}
 ]}
```

- **`motion batch variants.json`** makes each variant from the master: it applies its edits in order, each through the locks, then runs `check`. A variant whose edits are refused, or whose check has an error, is reported and not rendered; the others are rendered to `out/<id>.mp4` and saved as `out/<id>.json`.
- **A batch cannot edit `locks` or `roles`.** Those are the designer's, and a brief that needs them is not a variant.
- **One variant at a time.** A render already uses every core, so variants run in turn, and the master is read and its fonts loaded once.
- **The report** is one line per variant, and `--json` gives each variant's edits, check and output as data. `--check` stops before rendering. The command exits 1 when any variant is refused.

## Steps

Each step is one pull request, with its tests.

1. **Decisions.** D50 to D52 and this plan.
2. **Roles in the format.** `roles`, its check, `@role` edit targets and `motion roles` with `--json`.
3. **Roles at import.** `@role` names, the vocabulary, the report lines, measured on the corpus.
4. **Locks.** `locks`, the four kinds, refusals in `apply_edit`, the import's defaults.
5. **Batches.** `motion batch`, its manifest, report and `--json`.
6. **Docs.** `variants.md`, `edits-and-checks.md`, the glossary and the CHANGELOG.

## Out of scope

- **Beats and the style profile:** their own plan, read from the master's keys and labels.
- **Reframe, cut down, localise, reinterpret:** each needs layout or retiming on roles, and comes after.
- **Text limits** such as `max 18ch`: the role listing shows the master's length; a limit that `check` enforces waits for localise.
- **Parallel variants:** measured once batches exist; a render already fills the cores.
