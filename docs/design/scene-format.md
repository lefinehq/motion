Status: Draft · 2026-11-13

# Scene format

A motion scene is a small JSON file that describes a video: its size, length and layers, and what each layer does over time. It is built to be cheap for an agent to write. Defaults cover most values, and short strings say a lot in a few tokens.

## An example

```json
{"motion":1,"size":"1080p","fps":30,"dur":"5s","bg":"#0B0D12",
"assets":{"inter":"font:Inter","logo":"./logo.svg"},
"layers":[
{"id":"title","text":"Launch Week","font":"Inter Variable 96 700","at":"center 260","in":"fade-up 12f"},
{"id":"accent","rect":"240x8 r4","at":"center 340","fill":"#3B82F6 -> #8B5CF6 0-5s"},
{"id":"cards","repeat":5,"layout":"row gap 40","at":"center 700","data":{"label":["Speed","Scale","Search","Sync","Secure"]},"item":{"rect":"300x400 r24","fill":"#fff","shadow":"0 12 32 #0003","text":"{label}"},"in":"fade-up 60px 12f ease-out @15f stagger 6f"},
{"id":"bloom","fx":["glow 20 0.6"]}
]}
```

Here is what each layer does:

- **`title`** is a text layer. It fades up into place over 12 frames.
- **`accent`** is a thin bar whose colour shifts from blue to violet across the whole clip.
- **`cards`** is five cards in a row, one per label. They fade up one after another, starting at frame 15 and six frames apart.
- **`bloom`** has no content, only an effect. It adds a soft glow to everything below it.

The syntax shown here is illustrative, and `repeat`, `layout`, `data`, `item` and `fx` are designed but not built yet. What is built today is the canvas, `text` and `rect` layers, `fill`, `at`, `in`, `anim` and `presets`; the sections below say what is still planned.

## The canvas

The top of the file sets the canvas:

- **`size`:** a preset such as `1080p`, `4k` or `square`, or exact pixels such as `1920x1080`.
- **`fps`:** frames per second.
- **`dur`:** the length of the clip.
- **`bg`:** the background colour.

**Units.** Time is counted in frames (`12f`), and seconds are accepted (`0.4s`). Colours are hex. Numbers are kept to 4 significant digits.

## Layers

Layers are stacked from bottom to top, in the order they are listed.

| Layer | What it is |
|---|---|
| `text` | Text, with full shaping for any script |
| `rect` | A rectangle the size of its box, with an optional corner radius (`"300x100 r12"`) |
| `shapes` | Shape items: groups, rectangles, ellipses and free paths, drawn by fills and strokes (see [Shapes](#shapes-effects-and-compositions)) |
| `svg` | An SVG file as shapes |
| `image` | A picture file |
| `solid` | A flat colour |
| `comp` | A composition from `comps`, drawn in the layer's box at the layer's own time |
| `pivot` | An invisible layer (`"pivot":true`) that only carries a position, rotation and scale. Other layers can use it as a parent. |
| filter layer | A layer with only an `fx` list. It applies the effects to everything below it. |
| `camera` | The viewpoint for 2.5D layers |
| `audio` | A sound file placed on the timeline |

Video layers, lights and 3D models come in phase 2. See [Features](features.md).

Every layer can also have:

- **position and transform:** `at`, `anchor`, `scale`, `rotate`, `opacity`, plus `rotateX`, `rotateY` and `z` for depth
- **`parent`:** another layer to follow
- **`blend`:** a blend mode
- **`mask`:** cut-out shapes, with feather and expansion
- **`matte`:** another layer of the same list whose drawing is a stencil: `"matte":"reveal alpha"` shows the layer where `reveal` is opaque. The modes are `alpha` (the default), `alpha-inverted`, `luma` and `luma-inverted`; luma reads transparency as black. The matte layer is drawn as it would be at that moment, with its transform, parents, opacity, effects, span and its own matte. Whether it is also drawn in the frame is up to its `hidden`, and a hidden layer still mattes
- **`fx`:** a list of effects
- **`hidden`:** `true` leaves it out of the frame but keeps it in the scene; its children still move with it
- **`mblur`:** motion blur
- in and out points, and a time offset

## Properties

Every animatable value of a layer is a **property**. They are listed once, in `crates/motion-format/src/property.rs`; this table mirrors that list, and a test checks that the names match. Each property holds a typed value: a number, a point (`[x, y]`) or a colour ([D31](decisions.md#d31-typed-values-and-value-space-keys)).

| Property | Type | Rests at | Components | Meaning |
|---|---|---|---|---|
| `position` | point | where `at` puts it | `x`, `y` | Where the anchor sits in the parent, in px. |
| `anchor` | point | the centre of the layer's box | `anchor-x`, `anchor-y` | The point of the layer that sits at `position`, and that it scales and turns about, in px of its own box. |
| `scale` | point | `[1, 1]` | `scale-x`, `scale-y` | Size factor on each axis. One number scales both; negative values mirror. |
| `rotate` | number | 0 | | Rotation in degrees, clockwise. |
| `opacity` | number | 1 | | How opaque the layer is, 0 (invisible) to 1 (solid). |

**Resting values.** `at` sets where `position` rests, read in the parent's box. A layer may instead write `position` itself, an exact point in the parent's own coordinates, as imported scenes do; it cannot have both. Any other property rests where the layer sets it, by its name as a layer key, or at the value above:

```json
{"id":"diamond","rect":"500x500","at":"526 1254","rotate":45,"scale":0.8,"anchor":[242,109]}
```

**Components.** A track animates a whole property or one component of it: `"x"` moves only across, and `"scale"` animates both axes of scale. A property is animated whole or by its components, not both.

**Parenting and time.** A layer can move with another (`"parent":"card"`): its position, rotation and scale apply on top of the parent's, while opacity stays its own. Its `at` is then read in the parent's box, so `"at":"center center"` is the middle of the parent, and `motion at` prints where its anchor lands on the canvas. `"span":"1s-16s"` sets when a layer shows, on the scene's timeline (parenting moves a layer, not its time). `"offset":"1s"` says where the layer's own time starts: its keys and its entrance are in its own time, so moving a layer later is one edit. An offset may be negative, so a layer can start partway through its own time. `"stretch":2` plays its own time at half speed, and a negative stretch runs it backwards from `offset`. Speeds in `speed(...)` are per second of the layer's own time, so a stretch slows them too.

Checks measure the transformed box: the layer's box through its anchor, scale, rotation and position, and its parents'.

## Shapes, effects and compositions

A `shapes` layer draws a list of **items**. Each item is an object whose kind's name is one of its keys; that key holds the kind's main value, and the other keys are its properties and settings. Every kind has its own property registry, so its values and `anim` tracks follow the same rules as a layer's ([D32](decisions.md#d32-one-format-for-authored-and-imported-scenes)).

```json
{"id":"badge","at":"540 960","shapes":[
  {"group":[{"rect":[300,100],"round":50},{"fill":"#FFE000"}],"rotate":-4},
  {"ellipse":[40,40],"position":[-110,0],"id":"dot"},
  {"stroke":"#101010","width":6,"join":"round"}
]}
```

How items combine:

- **Order.** The first item draws on top.
- **Paints.** A fill or stroke draws every outline listed before it in its group, including the outlines inside earlier groups, seen through those groups' transforms. Above, the stroke outlines both the rounded rectangle and the dot; the fill paints only the rectangle.
- **Space.** A paint fills and strokes in its own group's space, so a stroke's width scales with the groups around it, not with the groups it reaches into.
- **Fill rule.** All the outlines one fill covers count as one path for its `rule`, so `evenodd` cuts holes where they overlap.
- **Groups.** A group has a layer's transform (`position`, `anchor`, `scale`, `rotate`, `opacity`), resting at the origin. Its opacity multiplies the paints inside it.
- **Box.** A shape layer's own coordinates start at its anchor, `[0, 0]`; `check` measures the outlines it draws, widened by half of each stroke.
- **Ids and `hidden`.** An item may have an `id`, unique within its layer; `"hidden":true` leaves it out of the frame.

Each kind lists its properties in `crates/motion-format/src/item.rs`; this table mirrors that list, and a test checks it.

| Item | Key holds | Properties | Settings |
|---|---|---|---|
| `group` | its items | `position`, `anchor`, `scale`, `rotate`, `opacity` | |
| `rect` | `size` | `size`, `position`, `round` | |
| `ellipse` | `size` | `size`, `position` | |
| `path` | `path` | `path` | |
| `fill` | `color` | `color`, `opacity` | `rule`: `nonzero`, `evenodd` |
| `stroke` | `color` | `color`, `opacity`, `width`, `miter` | `cap`: `butt`, `round`, `square`; `join`: `miter`, `round`, `bevel` |

`size` splits into `width` and `height`, and `position` into `x` and `y`, for tracks. A rectangle's `round` is capped at half its shorter side. A `path` is SVG path data with absolute `M`, `L`, `C` and `Z` (`"M0,0 C40,0 80,40 80,80 Z"`); a track on it uses a key list, and every key keeps the same number of vertices and the same closure, so the outline moves vertex by vertex.

**Effects.** `fx` is a list in the same item form, applied in order to what the layer draws:

| Effect | Key holds | Properties | Settings |
|---|---|---|---|
| `fill` | `color` | `color`, `opacity` | |

**Compositions.** `comps` holds compositions by id. Each has its own `size`, `dur`, optional `fps` (the scene's by default) and `bg` (clear by default), and its own `layers`. A layer with `"comp":"card"` draws it in a box of the composition's size. The composition's time is the layer's own time, so `offset` and `stretch` move and slow it, and the layer shows nothing outside the composition's length. Compositions may draw other compositions, but never themselves.

```json
"comps":{"card":{"size":"600x400","dur":"5s","layers":[{"id":"title","text":"Hello"}]}},
"layers":[{"id":"card-1","comp":"card","at":"540 700","offset":"1s"}]
```

## Animation, at three levels of detail

All three levels compile to the same thing inside the engine: a **track** of keys for each property. Use the simplest level that does the job.

**1. Behaviours.** These say what happens in a few words:

```json
"in":"fade-up 60px 12f ease-out @15f stagger 6f"
```

**2. Property tracks.** These animate one property, in the `anim` object. The string form goes from A to B:

```json
"anim":{"x":"0 -> 200 @0-30f ease-out"}
```

The range after `@` takes `f` (frames) or `s` (seconds). The curve is optional and defaults to `linear`. Values outside the property's range are rejected.

**3. Keyframes.** These give exact values at exact times, as a list of `[time, value]` or `[time, value, curve]`:

```json
"anim":{"rotate":[[0,-10],["1s",0,"cubic(.2,0,0,1)"],["2s",5]]}
```

A value is written in its property's type: `0.5`, `[540, 960]` or `"#FFE000"`. A key can also be an object, which bends the path of a point that moves in space (`position`, `anchor`):

```json
"anim":{"position":[{"t":0,"v":[540,1800],"curve":"ease-out","path":[[0,-200],[-120,0]]},{"t":"1s","v":[540,960]}]}
```

`path` holds two offsets: the handle leaving this key, from its value, and the handle arriving at the next key, from that key's value. Without it the path is straight. A point that moves in space takes one curve, which times the whole move along the path; a point that does not (`scale`) can take one curve per component, `["ease-out","linear"]`.

A bare time is frames; a string such as `"1s"` carries its unit. A key may fall before the layer's own zero (`-6` or `"-0.2s"`). Keys go in time order. The curve on a key leads to the next key. The last key's curve is unused. Before its first key a property holds the first value, and after its last key the last value.

We store the highest level that fits. A delay or a stagger therefore stays a single value, and changing it is a single edit.

### Curves

A curve is a cubic bézier, as in CSS, or `hold`. The names are shortcuts from one table:

| Name | What it does |
|---|---|
| `linear` | Constant speed. |
| `hold` | Keeps the start value, then jumps to the end value at the end. |
| `ease` | Starts and ends gently. |
| `ease-in` | Starts slowly. |
| `ease-out` | Ends slowly. |
| `ease-in-out` | Starts and ends slowly. |

`cubic(x1,y1,x2,y2)` sets the curve exactly, for example `cubic(.2,0,0,1)`. The x values must be between 0 and 1, so time only moves forward. The y values may go outside 0 to 1, which makes the value overshoot (`cubic(.3,1.6,.5,1)`) or wind up first.

`speed(out,out-reach,in,in-reach)` sets the speed leaving the key and arriving at the next, in the value's units per second (pixels per second along the path for a moving point), each with how far into the span its handle reaches, 0 to 1. `speed(0,.33,0,.33)` eases in and out over a third of the span on each side. Unlike `cubic`, it can hold a span whose two keys are equal, where the value leaves and comes back.

### Presets

A preset is data: a list of properties and where each starts. The behaviour then animates each one from its start to its resting value. These are built in (`crates/motion-format/src/presets.json`):

| Preset | Starts from |
|---|---|
| `fade` | opacity 0 |
| `fade-up` | opacity 0, moved down by the distance, so it rises into place |
| `fade-down` | opacity 0, moved up by the distance |
| `fade-left` | opacity 0, moved right by the distance |
| `fade-right` | opacity 0, moved left by the distance |
| `pop` | opacity 0, scale 0.6 |
| `spin-in` | opacity 0, rotated -90 degrees |

A start is a number, `"+d"` (the resting value plus the behaviour's distance) or `"-d"` (minus it). A scene adds presets, or overrides a built-in one by reusing its name, with a `presets` key:

```json
"presets":{"drop":{"opacity":0,"y":"-d","scale":0.8}},
"layers":[{"id":"title","text":"Hi","in":"drop 80px 12f ease-out"}]
```

### One writer per property

If `in` already animates a property, `anim` may not also animate it: the file is rejected, naming the property and the preset. There are two ways out. Move the entrance into `anim`: set `in=none` and write the preset as `anim` tracks, one per property it sets; the error message gives that edit for the layer, computed from the preset, the layer's `at` and the scene size (custom curves are shown to two decimals). Or define a scene preset under `presets` that leaves the property out. For example `"in":"fade 10f"` with `"anim":{"opacity":"0 -> 1 @0-12f"}` is an error; `anim` on `rotate` is fine.

## Repeats and data

`repeat` makes several copies of an `item`, and `data` fills in what differs between them. The five cards above are one layer, not five.

The copies get numbered ids starting at 1: `cards.1` to `cards.5`, also reachable as `card1` to `card5`.

## Layout

A layer or group can lay out its children in rows, columns or grids, for example `row gap 40`, `col`, `grid 3 gap 24` or `align center`.

Layout is resolved before anything moves. Every animation is therefore relative to where its element sits in the layout.

## Text

A text layer sets its words (`text`, where `\n` ends a paragraph) from font assets ([D33](decisions.md#d33-text-is-shaped-from-font-assets)). `fonts` at the top of the scene lists font files, folders and locks, relative to the scene file; Inter ships built in as the family `Inter Variable`. `font` names a family or a PostScript name, a size and a weight: `"Inter Variable 96 700"` or `"Lato-Black 55"`. A font the scene does not have is an error that lists the faces there are, never a silent fallback.

```json
"fonts":["fonts/lato"],
"layers":[{"id":"title","text":"Keep\nyour life\nsimple","font":"Lato-Black 55","fill":"#FFFFFF",
  "style":{"caps":"all","align":"center","stroke":"#8F8F8F","stroke-width":2,"leading":66},
  "runs":[{"range":[0,4],"fill":"#FFE000"}]}]
```

- **`style`** sets character and paragraph keys for the whole text. `font` and `fill` stay layer keys.
- **`runs`** override character keys over `range`, `[start, end]` in characters; a later run wins where runs overlap.
- **`paragraphs`** holds one object of paragraph keys per paragraph, in order.
- **`box`** makes box text, which wraps inside the box: `[w, h]` from the layer's origin, or `[x, y, w, h]` from the box's top-left corner in the layer's own coordinates, as imported box text keeps it. Without it the text is point text: each paragraph is one line, aligned about the origin, and the origin is where the first line's baseline starts.

The layer's box is the box its lines cover, which for point text starts above and maybe left of the origin; `anchor` rests at its centre, so `at` centres the text. The keys are listed once, in `crates/motion-format/src/text.rs`; this table mirrors that list, and a test checks it.

| Style key | Level | Accepts |
|---|---|---|
| `font` | character | `<family or PostScript name> <size> [<weight>]`, e.g. `Lato-Black 55` |
| `italic` | character | `true` or `false` |
| `fill` | character | a colour, e.g. `#FFFFFF` or `[1, 1, 1, 1]` |
| `stroke` | character | a colour, e.g. `#8F8F8F` |
| `stroke-width` | character | a number of px, at least 0 |
| `paint` | character | `fill-over` or `stroke-over` |
| `tracking` | character | a number, in thousandths of an em |
| `leading` | character | `auto` or a number of px |
| `baseline` | character | a number of px |
| `caps` | character | `normal`, `small`, `all` or `all-small` |
| `faux-bold` | character | `true` or `false` |
| `faux-italic` | character | `true` or `false` |
| `align` | paragraph | `left`, `right`, `center`, `justify-left`, `justify-right`, `justify-center` or `justify-all` |
| `indent-first` | paragraph | a number of px |
| `indent-start` | paragraph | a number of px |
| `indent-end` | paragraph | a number of px |
| `space-before` | paragraph | a number of px |
| `space-after` | paragraph | a number of px |
| `auto-leading` | paragraph | a number, at least 0 |

`faux-bold` thickens the glyphs and adds 30 thousandths of an em after each character, the space the project files' own layouts give it.

### Text animators

`animate` on a text layer lists animators. An animator's key holds its selectors, and its other keys are what it applies: each part of the text takes those values scaled by how much the selectors choose it. An animator applies only the properties it sets or animates.

```json
"animate":[{"animator":[{"range":"line","anim":{"offset":[{"t":"0.48s","v":0},{"t":"2.76s","v":1,"curve":"ease-out"}]}}],"position":[0,75],"opacity":0}]
```

Above, every line starts 75 px low and invisible. As `offset` moves the range off the lines, from the first line to the last, each line rises and fades in.

- **Selectors.** `range` counts characters (`char`), characters other than spaces (`char-no-space`), `word`s or `line`s; its key holds which. `start`, `end` and `offset` are fractions of the parts, from 0 (before the first) to 1 (after the last). `amount` scales the choice, and a negative amount reverses the animator. With the `square` shape, a part is chosen by how much of it the range covers, and `smooth` 0 makes that all or nothing, decided by the part's middle. The other shapes are read at each part's middle: `ramp-up` rises across the range and stays chosen after it, `ramp-down` is the reverse, and `triangle`, `round` and `smooth` rise and fall inside it. `mode` combines a selector with the ones before it.
- **What applies.** `position` and `rotate` add. `scale` and `opacity` move from 1 toward the animator's value. `fill`, `stroke` and `stroke-width` move from the part's own toward the animator's. `tracking` adds space after each chosen part, and the parts after it on its line move along, keeping the line's alignment.
- **Each part's anchor** is the middle of its baseline, moved by the animator's `anchor`; parts scale and turn about it.

| Animator | Key holds | Properties | Settings |
|---|---|---|---|
| `animator` | its items | `position`, `anchor`, `scale`, `rotate`, `opacity`, `fill`, `stroke`, `stroke-width`, `tracking` | |

| Selector | Key holds | Properties | Settings |
|---|---|---|---|
| `range` | `by` | `start`, `end`, `offset`, `amount`, `smooth` | `shape`: `square`, `ramp-up`, `ramp-down`, `triangle`, `round`, `smooth`; `mode`: `add`, `subtract`, `intersect`, `min`, `max`, `difference` |

## Text motion

Text can be animated one character, one word or one line at a time:

```json
"in":"type-on char 2f"
"in":"rise word stagger 3f"
```

## Modifiers

Modifiers add motion that would otherwise need keyframes or code:

- **`jitter freq amp seed`:** a repeatable random shake.
- **`loop`:** repeats an animation.
- **`link title.y *0.5 +20`:** follows another value.
- **`follow`:** trails another layer.

Modifiers are declarative and repeatable, so checks and repeatability keep working. motion does not run arbitrary scripts inside scenes (see [D10](decisions.md#d10-modifiers-instead-of-scripts)).

## Fonts and assets

motion never goes to the network while rendering. A render depends only on the scene, the files it points to, and the motion version.

- **Inter** is built in.
- **Other fonts, images and audio** are referenced by file path.
- **`motion fonts fetch <scene>`** finds every face the scene's text uses and its `fonts` lack, in open-licensed sources, and pins each to an exact file in a lock beside the scene (`story.fonts.lock` for `story.json`). It adds the lock to `fonts` and downloads the files into the font cache (`$MOTION_CACHE`, or the platform's cache folder). It is a separate, explicit step; `motion import --fetch-fonts` runs it for an imported project ([D39](decisions.md#d39-fonts-are-resolved-before-a-render)).
- **A lock in `fonts`** loads each face it pins from the cache and checks the file's hash. A face the cache lacks is an error that names `motion fonts fetch`.
- **System fonts** work, but the checker warns about them, because the result then depends on the machine.
- **A missing font or glyph** is an error, never a silent fallback.

## Validation

- **JSON Schema.** A schema is generated from the engine's own types and published, so editors and agents can validate scenes.
- **Errors you can act on.** When a short string does not parse, the error shows what the full, expanded form should look like.

## Versions

`"motion": 1` is the format's version. It stays 1 while changes only add: a new key, a new value form, a looser limit. A change that alters what a written scene means bumps it, and motion reads every older version through migrations ([D36](decisions.md#d36-the-format-only-changes-in-the-open)). The scenes in `crates/motion-format/tests/compat/` hold that promise in CI: together they use every name in this document, and what each one draws is a snapshot.

## Why JSON

We measured the same scene in several formats (see the [research note](../research/2026-11-13-baselines.md#formats)). JSON wins on three counts:

- **Density matters more than syntax.** Defaults, presets and short strings cut tokens about 11 times. Switching between JSON, YAML and a custom language changed the count by only 3–15%.
- **Models write JSON reliably.** Published studies find that models produce valid JSON more reliably than YAML.
- **The tooling already exists.** JSON Schema and constrained output come for free.

The short strings inside the JSON look like CSS on purpose, because models already know that style.
