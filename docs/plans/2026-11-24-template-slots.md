Status: Draft · 2026-11-18

# Every template variable

**Goal:** `nakkash import` names, for every `.aep` and `.mogrt` template, the values a designer would vary: its words, its palette, its logo and footage, its timing and its controls, each a slot with a role ([D77](../design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary)). A variant is then a list of slot values, and `nakkash vary` builds on that list.

```text
$ nakkash import intro.aep --comp SHOT02 -o intro.json
  slots: 4 text, 5 colour, 1 image, 1 timing
$ nakkash slots intro.json
scene
  $title text "SPEED"  "SAMPLE in 01_EDIT_TEXT_and_LOGO"  title  (1 value follows)
  $url text "www.logosample.com"  "www.logosample.com in 02_EDIT_TEXT_UNDER"  url  (1 value follows)
  $background color #000000  "black #000000"  background  (3 values follow)
  $accent color #E63946  "red #E63946"  accent  (41 values follow)
  $logo image "(Footage)/IMG/logo.png"  "logo.png in 02_EDIT_TEXT_and_LOGO"  logo  (1 value follows)
  $duration number 5  "Duration in seconds"  duration  (1 value follows)
$ nakkash edit intro.json '$accent=#2A9D8F'
```

## What the corpus says

Measured on the 198 corpus templates with the import before this plan:

| | Corpus |
|---|---|
| Templates with no slot | 95 of 198 |
| Text layers | 998, of which 12 follow a slot |
| Distinct colours drawn (8-bit) | 1,004; median 4 a template, 90th percentile 9, at most 46 |
| Colour clusters within 0.04 in OKLab | 921 |
| Layers drawing footage | 520, of 297 files |
| Footage the import cannot draw | 52 stills and 67 clips it cannot find, 32 clips in a codec without a decoder |

## The data shape

This is the contract `nakkash vary` reads.

### In the scene

Slots are declared on the scene, so every composition sees them ([D44](../design/decisions.md#d44-slots-are-named-values-that-layers-follow)):

```json
"slots":{
  "title":{"text":"SPEED","label":"SAMPLE in 01_EDIT_TEXT_and_LOGO","role":"title"},
  "url":{"text":"www.logosample.com","label":"www.logosample.com in 02_EDIT_TEXT_UNDER","role":"url"},
  "background":{"color":"#000000","label":"black #000000","role":"background"},
  "accent":{"color":[0.901961,0.223529,0.27451,1],"label":"red #E63946","role":"accent"},
  "logo":{"image":"(Footage)/IMG/logo.png","label":"logo.png in 02_EDIT_TEXT_and_LOGO","role":"logo"},
  "photo":{"image":null,"label":"photo.jpg in EDIT_PHOTO, not found","role":"image"},
  "duration":{"number":5,"label":"Duration in seconds","role":"duration"}
}
```

- **Types.** `text`, `color`, `number` and `point` as before, and two new ones: `image`, a picture or vector footage file, and `video`, a clip; either may be `null`, which draws nothing.
- **`role`** says what the slot is for. Text: `title`, `subtitle`, `body`, `label`, `url`, `email`, `phone`, `date`, `price`, `cta`, `number`, `text`. Colour: `background`, `text`, `accent`, `light`, `dark`, `grey`. Media: `logo`, `image`, `video`, `texture`. Timing: `duration`. A control's slot (D44, D53) has no role unless its colour became a palette entry.
- **Ids** are the role, numbered in reading order (`title`, `title-2`); a control keeps the id its name gives it.

Values follow them as before:

```json
{"id":"sample","text":"$title","font":"Montserrat-Bold 180","fit":"shrink 1240x310"}
{"id":"bar","rect":"400x20","fill":"$accent l-0.021 c+0.004 h+1.6"}
{"id":"glow","rect":"400x20","fill":"$accent / 0.5"}
{"id":"logo-png","image":"$logo","box":[3840,800],"fit":"contain"}
```

- **A derived colour** is a reference followed by a change in OKLCH, each part optional, in this order: `l±n` adds to the lightness (0 to 1), `c±n` to the chroma, `h±n` degrees to the hue, and `/ a` sets the alpha (0 to 1). The result is clipped into sRGB. Only a colour slot takes a change.
- **`fit` on a text layer.** `"shrink WxH"`: when the lines are wider than `W` or taller than `H`, every size in the text scales down by the largest factor that makes them fit, about the vertical middle of the lines unscaled; alignment holds horizontally. `"wrap WxH"`: point text breaks its lines at `W`, as box text does, each line aligned about the origin as point text is; the first baseline stays, and the text shrinks when it is taller than `H`. `"shrink"` alone: box text shrinks until its lines fit its box. A text never grows.
- **`fit` on an image or video layer.** `"fill"` (the default) stretches the picture over the box, `"contain"` keeps its proportions and centres it inside the box, `"cover"` keeps them and fills the box, cut by it.
- **`dur`** of the scene may follow a number slot, in seconds, and `labels` are written from the project's composition markers.

### From `nakkash slots --json`

One entry per slot, as now, with `role` and what the vary command needs to keep a value inside the design:

```json
[{"comp":null,"id":"title","type":"text","value":"SPEED","label":"SAMPLE in 01_EDIT_TEXT_and_LOGO","role":"title",
  "followers":["comps.comp-01-edit-text-and-logo.layers[sample].text"],
  "text":{"font":"Montserrat-Bold","size":180,"fit":"shrink 1240x310","characters":5,"lines":1},
  "edit":true},
 {"comp":null,"id":"accent","type":"color","value":[0.901961,0.223529,0.27451,1],"label":"red #E63946","role":"accent",
  "followers":["layers[bar].fill","…"]},
 {"comp":null,"id":"logo","type":"image","value":"(Footage)/IMG/logo.png","label":"logo.png in 02_EDIT_TEXT_and_LOGO","role":"logo",
  "followers":["comps.comp-02-edit-text-and-logo.layers[logo-png].image"],
  "media":{"box":[3840,800],"fit":"contain"},
  "edit":true}]
```

- `text`: the first follower's font, size and fit, and the original's characters and lines.
- `media`: the first follower's box and fit.
- `edit`: a follower sits in an edit composition.
- `--coverage` prints, as data with `--json`, the text layers, colours and footage no slot holds: `{"text":[0,12],"colors":[1,18],"media":[0,3]}` (unbound, total) and where each unbound one is.
- `--verify` prints, for each slot, the change it made, the frame it rendered and how many pixels moved; `--json` gives the same as data, and the command exits 1 when a slot moves none.

A variant is then a list of edits, `$title="SPEED DIGITAL"`, `$accent=#2A9D8F`, `$logo=brand/logo.png`, through the locks as before ([D51](../design/decisions.md#d51-locks-are-refused-in-the-edit)).

## How the import finds them

1. **Text.** Each text layer becomes a slot, except a copy of a text already met in the same composition with the same font, which follows that text's slot. The role, in order of strength: a name or `@role` prefix ([D50](../design/decisions.md#d50-roles-name-what-a-masters-layers-do)); the words (`www.`, a domain, `@` and a domain, a phone number, a date, a price, digits alone); the size, the largest text of its composition being its `title` and the next its `subtitle`, a text of several lines its `body`, and a small one its `label`. The fit is measured from the lines the scene's own layout gives the original, so the original never shrinks.
2. **Colour.** Every colour place the format types as a colour is a use. Uses cluster in OKLab, heaviest first, within 0.04; each use then follows its entry, with its difference. A colour control's slot whose value is in a cluster is that cluster's entry. Naming: the entry of `bg` or of the largest layer at the bottom is `background`; the one most text uses is `text`; the others by weight, `accent` for a chroma above 0.04, else `light`, `dark` or `grey`. The fill of an adjustment layer draws nothing, so it is not a use.
3. **Media.** Each footage file becomes a slot that each layer drawing it follows; a file the import cannot draw is a slot of `null`. The role: `logo` from the file's, layer's or composition's name (`logo`, `brand`, and a sample logo's name), or from an edit composition; `image` and `video` for footage in an edit composition or named as a placeholder; `texture` for footage blended over others; the kind otherwise.
4. **Timing.** `duration` from the root's length; `labels` from composition markers within the composition's length, named from the marker's comment, chapter or cue point as a label id (`Logo In` → `logo_in`, a leading digit as `m_3`), or `m1`, `m2` by order; a marker that lasts gives its end too, as `<name>_end`: `"<name>+0.5s"`.
5. **Edit compositions** are found by name (`EDIT`, `CHANGE`, `REPLACE`, `YOUR`, `PLACE`, `INSERT`, `CUSTOMIZE`), and every slot followed in one is marked and listed first.

## Steps

Each step is one pull request, with its tests.

1. **Decision.** D77, this plan and the format's new keys in the scene format, so `nakkash vary` can be written against them.
2. **Format and engine.** `image` and `video` slot types and `null`; `role`; derived colour references; references in `dur`; `fit` on text, laid out by the typesetter; `fit` on images and videos, drawn by the renderer; an image or video layer of no file.
3. **Import.** Text, palette, media and timing slots, edit compositions, the report line, no default locks; measured on the corpus.
4. **Listing and proof.** `nakkash slots` with roles, `--coverage` and `--verify`.
5. **Export.** Resolved references in the plan, so a slot's value goes back at every value that follows it. Text stays out until the export can lay a text out as the project does.
6. **Docs.** `variants.md`, `aep-import.md`, the guide and the changelog, with the corpus numbers before and after.

## Out of scope

- Motion as slots (easing, speed, stagger): with the style profile.
- Growing a text into free space, and reflowing a layout around a longer text.
- Retiming keys inside compositions; a variant retimes with `dur` and the labels.
- Writing a text back into its text document, a new footage file into a project's footage item, and audio slots.
