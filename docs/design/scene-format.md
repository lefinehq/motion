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

The syntax shown here is illustrative, and `repeat`, `layout`, `data`, `item` and `fx` are designed but not built yet. What is built today is the canvas, `text`, `rect`, `image` and `video` layers, `fill`, `at`, `in`, `anim` and `presets`; the sections below say what is still planned.

## The canvas

The top of the file sets the canvas:

- **`size`:** a preset such as `1080p`, `4k` or `square`, or exact pixels such as `1920x1080`.
- **`fps`:** frames per second.
- **`dur`:** the length of the clip.
- **`bg`:** the background colour.

**Units.** Time is counted in frames (`12f`), and seconds are accepted (`0.4s`). Colours are hex. Numbers are kept to 4 significant digits.

## Layers

Layers are stacked from bottom to top, in the order they are listed.

Every layer has an `id`: lowercase letters, digits, `-` and `_`, starting with a letter, and unique in its list. `scene` is reserved, because it names the scene itself in an address. A layer anywhere in a scene is addressed by the ids down to it, joined by `/`: `card/title` is the layer `title` inside the comp that `card` shows ([D43](decisions.md#d43-one-layer-address-and-edits-per-instance)).

| Layer | What it is |
|---|---|
| `text` | Text, with full shaping for any script |
| `rect` | A rectangle the size of its box, with an optional corner radius (`"300x100 r12"`) |
| `shapes` | Shape items: groups, rectangles, ellipses and free paths, drawn by fills and strokes (see [Shapes](#shapes-effects-and-compositions)) |
| `svg` | An SVG file as shapes |
| `image` | A PNG or JPEG file stretched over the layer's `box` (see [Images](#images)) |
| `video` | A video clip played in the layer's own time and stretched over its `box` (see [Videos](#videos)) |
| `solid` | A flat colour |
| `comp` | A composition from `comps`, drawn in the layer's box at the layer's own time; `overrides` changes its layers for this layer alone |
| `pivot` | An invisible layer (`"pivot":true`) that only carries a position, rotation and scale. Other layers can use it as a parent. |
| filter layer | A layer with only an `fx` list. It applies the effects to everything below it in its composition (see `adjust`). |
| `camera` | The viewpoint for 2.5D layers |
| `audio` | A sound file placed on the timeline |

Lights and 3D models come in phase 2. See [Features](features.md).

Every layer can also have:

- **position and transform:** `at`, `anchor`, `scale`, `rotate`, `opacity`, plus `rotateX`, `rotateY` and `z` for depth
- **`parent`:** another layer to follow
- **`blend`:** a blend mode
- **`mask`:** cut-out shapes, with feather and expansion
- **`matte`:** another layer of the same list whose drawing is a stencil: `"matte":"reveal alpha"` shows the layer where `reveal` is opaque. The modes are `alpha` (the default), `alpha-inverted`, `luma` and `luma-inverted`; luma reads transparency as black. The matte layer is drawn as it would be at that moment, with its transform, parents, opacity, effects, span and its own matte. Whether it is also drawn in the frame is up to its `hidden`, and a hidden layer still mattes
- **`fx`:** a list of effects
- **`adjust`:** `true` applies the layer's `fx` to everything below it in its composition instead of to its own content, which is not drawn but says where they apply, by its alpha. A filter layer, with no content, applies them everywhere. The change is mixed in by the layer's opacity and matte; points and lengths of its effects are in the layer's own coordinates ([D49](decisions.md#d49-effects-that-read-pixels-run-on-a-canvas-of-their-own))
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

**Labels and relative times.** A composition, the scene's root included, may name moments of its timeline: `"labels":{"intro":"0.5s","cta":"6s","outro":"cta+2s"}`. A label name is a lowercase letter followed by lowercase letters, digits and `_` (no `-`), and a label is a time or another label of the same composition moved by a time, in any order but not in a circle. A layer's `offset` and the ends of its `span` may then be written against a base:

| Written | Means |
|---|---|
| `2s`, `-6f` | an absolute time |
| `cta`, `cta+12f`, `cta-0.5s` | a label of the layer's composition, moved |
| `<`, `<+6f` | where the layer before it in the list ends, moved |
| `<<`, `<<-3f` | where the layer before it starts, moved |

```json
{"id":"title","text":"Hello","span":"intro-cta"},
{"id":"card","comp":"card","offset":"<-6f"},
{"id":"cta","text":"Try it","span":"cta-outro+1s"}
```

- The layer before starts at its span's start, or at its `offset` without a span. It ends at its span's end; without one, a `comp` layer ends when its composition does at its stretch, and any other layer at the end of the timeline. The first layer has no layer before it.
- In a span, a `-` right after a base followed by a number moves the base; otherwise it separates the ends. `intro-6f-cta` runs from six frames before `intro` to `cta`, and `intro-2s`, which would have no end the other way, from `intro` to 2 s.
- A base and its move add in the unit they share, else in frames.
- Times resolve when the scene is read; the file keeps what was written, so moving a label or a layer moves everything written against it.

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
- **Trims.** A `trim` cuts every outline listed before it in its group, the outlines inside earlier groups included, down to a stretch of its length. It changes the outline itself, so every fill and stroke that draws the outline draws the cut one, wherever it sits in the list. Trims inside a group cut before those of the groups around it, and those in one group cut in order.
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
| `trim` | `end` | `end`, `start`, `offset` | `mode`: `each`, `joined` |

`size` splits into `width` and `height`, and `position` into `x` and `y`, for tracks. A rectangle's `round` is capped at half its shorter side. A `path` is SVG path data with absolute `M`, `L`, `C` and `Z` (`"M0,0 C40,0 80,40 80,80 Z"`); a track on it uses a key list, and every key keeps the same number of vertices and the same closure, so the outline moves vertex by vertex.

A `trim` keeps the stretch from `start` to `end`, shares of the length from 0 to 1 (`{"trim":0.5}` keeps the first half), swapped if `start` is after `end`. `offset` moves the stretch along in degrees, 360 once around; a stretch that runs past the end goes on around a closed outline as one line and continues an open one as a second piece. `each` cuts every outline the same way, `joined` cuts them as one line in order. Equal `start` and `end` draw nothing, not even a cap. A cut closed outline is open: a stroke ends in its caps, and a fill closes it with a straight line. Lengths are arc lengths in the space each outline sits in.

**Effects.** `fx` is a list in the same item form, applied in order to what the layer draws:

| Effect | Key holds | Properties | Settings |
|---|---|---|---|
| `fill` | `color` | `color`, `opacity` | |
| `transform` | `scale` | `position`, `anchor`, `scale`, `rotate`, `opacity` | |
| `tint` | `amount` | `black`, `white`, `amount` | |
| `hue-saturation` | `hue` | `hue`, `saturation`, `lightness`, `colorize-hue`, `colorize-saturation`, `colorize-lightness` | `mode`: `adjust`, `colorize` |
| `exposure` | `exposure` | `exposure`, `offset`, `gamma` | `space`: `linear`, `gamma` |
| `levels` | `gamma` | `input-black`, `input-white`, `gamma`, `output-black`, `output-white` | `channel`: `rgb`, `red`, `green`, `blue`, `alpha` |
| `black-white` | `tint` | `reds`, `yellows`, `greens`, `cyans`, `blues`, `magentas`, `tint`, `tint-color` | |
| `curves` | `rgb` | `rgb`, `red`, `green`, `blue`, `alpha` | |
| `invert` | `channel` | `blend` | |

A `transform` moves everything the layer draws, inside the layer, as a group's transform moves its items: `anchor` (px of the layer's own coordinates) goes to `position`, scaled and turned about it, and `opacity` multiplies the layer's. Both points rest at the origin, so `{"transform":1.5}` scales about the layer's origin; an import writes both. Several apply in order, each moving what the ones before it moved.

The colour effects change each pixel on its own, on its straight colour, levels from 0 to 1. `tint` maps each colour's luma (Rec. 601) from `black` to `white` and mixes that in by `amount`. `hue-saturation` turns hues by `hue` degrees and moves saturation and lightness towards their ends by shares from -1 to 1; in `colorize`, every colour takes the `colorize-` hue and saturation at its own luma. `exposure` multiplies light by 2 to the `exposure`, adds `offset` and raises it to 1/`gamma`, on light decoded from sRGB in `linear` space. `levels` maps its channel from `input-black`–`input-white` to `output-black`–`output-white` through a power of 1/`gamma`. `black-white` turns each colour to grey: its lowest level, plus what its highest has over its middle times the share of its primary (`reds`, `greens`, `blues`), plus what its middle has over its lowest times the share of the secondary the two make (`yellows`, `cyans`, `magentas`); `tint` mixes in the hue and saturation of `tint-color` at that grey. Each `curves` property is a path of up to 16 points from input to output level in rising input, `"M 0,0 L 0.5,0.6 L 1,1"`, joined by a natural cubic spline and flat beyond its ends; a channel's own curve applies before `rgb`, and an empty path changes nothing. `invert` turns each level of its channel (`rgb`, `red`, `green`, `blue` or `alpha`) into 1 minus itself, keeping `blend` of the original.

**Compositions.** `comps` holds compositions by id. Each has its own `size`, `dur`, optional `fps` (the scene's by default), `bg` (clear by default) and `labels`, and its own `layers`. A layer with `"comp":"card"` draws it in a box of the composition's size. The composition's time is the layer's own time, so `offset` and `stretch` move and slow it, and the layer shows nothing outside the composition's length. Compositions may draw other compositions, but never themselves.

```json
"comps":{"card":{"size":"600x400","dur":"5s","layers":[{"id":"title","text":"Hello"}]}},
"layers":[{"id":"card-1","comp":"card","at":"540 700","offset":"1s"}]
```

**Overrides.** A `comp` layer may change the layers of the composition it draws, for itself alone, with `overrides`: layer keys by the address of the layer they go on, relative to the composition ([D43](decisions.md#d43-one-layer-address-and-edits-per-instance)). Each override's keys replace the same keys of that layer, and the result is checked as any layer is, so an error points at the inner layer, `card-2/title.text`. An address may reach deeper, through the composition's own `comp` layers: `badge/icon`. Layers without overrides share the composition; an override cannot change a layer's `id`.

```json
"layers":[{"id":"card-1","comp":"card"},
          {"id":"card-2","comp":"card","overrides":{"title":{"text":"Sale"},"badge/icon":{"fill":"#E63946"}}}]
```

## Images

An image layer draws a picture file: `image` names a PNG or JPEG file, relative to the scene file (an absolute path also works), and `box` says where its pixels go in the layer's own coordinates, `[width, height]` from the origin or `[x, y, width, height]`, as for box text. The picture is stretched to fill the box, so its size in the file does not have to match; an import writes the size the project gave it.

```json
{"id":"photo","image":"photos/beach.jpg","box":[1920,1080],"at":"center center","opacity":0.8}
```

- **Box.** `box` is required: the scene is read, checked and laid out without opening its files, so the layer's size is in the scene. `anchor` rests at the box's centre, and `check` measures the box.
- **Files.** A render reads and decodes each file once, the first time a frame shows it, and draws every later frame from that. A file that is missing, or is not a PNG or JPEG file, is an error that names the file; a file no frame shows is never read. The format is recognised by the file's first bytes, not its name.
- **Colour.** PNG keeps its transparency; JPEG is opaque. Pixels are read as sRGB, and sampled bilinearly.
- Like any layer, an image has a transform, parent, opacity, span and time, can be a matte, and can be matted. Video files are not image layers; they are [video layers](#videos).

## Videos

A video layer plays a clip: `video` names an `.mov` or `.mp4` file, relative to the scene file, and `box` says where its frames go, as for an image ([D47](decisions.md#d47-video-layers-play-their-clip-in-their-own-time)).

```json
{"id":"sparks","video":"footage/sparks.mov","box":[1920,1080],"offset":"2s","opacity":0.8}
```

- **Time.** The clip plays in the layer's own time: at its own time `t`, the layer draws the frame showing `t` seconds into the clip. So `offset` starts it later and `stretch` slows it (`2`) or speeds it (`0.5`). Before the clip starts and from its end on, the layer draws nothing.
- **Codecs.** QuickTime Animation (`rle `, RGB, or ARGB with its alpha) and Photo JPEG decode today; a clip in any other codec is an error that names the file and the codec. The container is read by motion itself, with no system library.
- **Files.** A render opens each clip once and decodes frames as they are shown, keeping the last ones, so a render going forward decodes each frame once. A file no frame shows is never read.
- Like any layer, a video has a transform, parent, opacity, span and time, can be a matte, and can be matted. Its sound is not played: scenes have no audio yet.

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

The string form may chain values, which split the span evenly; each step takes the curve. A value between the first and the last may say where it falls, as a percentage of the span, and the others share what is left:

```json
"anim":{"x":"0 -> 200 -> 150 @0-30f out-quad","y":"0 -> 80@25% -> 40 -> 0 @0-1s"}
```

A value may be written against the property's resting value on its layer or item: `rest`, `rest+<value>`, `rest-<value>` (a value of the property's type, such as `rest+0,-40` for a point) or `rest*<factor>`. It works on numbers and points, in the string form and in key lists, and resolves against the rest after layout, so a move written from `rest` follows the layer when `at`, its parent or its text changes:

```json
"anim":{"y":"rest+60 -> rest @0-12f out-cubic","scale":"rest*0.8 -> rest @0-12f"}
```

A relative value is not range-checked when the scene is read, since the rest may be known only once text is measured; the property's range clamps it when it is drawn.

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

A curve is a cubic bézier, as in CSS, a family's curve, steps, or `hold`. The names are shortcuts from one table:

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

**Families.** `<shape>-<family>` names one of the classic curves, such as `out-expo` or `in-out-quint`. Each starts at exactly 0 and ends at exactly 1, and is evaluated exactly at every frame.

- **Shapes** run a family's curve `f` four ways:
  - `in` starts slowly;
  - `out` ends slowly (`1 - f(1 - t)`);
  - `in-out` does `in` over the first half and `out` over the second;
  - `out-in` does the reverse.
- **Families:**

  | Family | Curve |
  |---|---|
  | `quad`, `cubic`, `quart`, `quint` | Powers 2 to 5 of time. |
  | `sine` | A quarter of a cosine. |
  | `circ` | A quarter circle. |
  | `expo` | Exponential, `(2^(10t) - 1) / 1023`. |
  | `back` | Winds up before it goes, or overshoots on the way out. |
  | `elastic` | Swings past the end and settles, like a spring. |
  | `bounce` | Bounces up to the end four times. |

- **Parameters:**
  - Any power: `in-out(2.5)`.
  - Back's overshoot: `out-back(2)`; 1.70158 by default, about 10%.
  - Elastic's amplitude (at least 1) and period (above 0, at most 2): `out-elastic(1.2,.4)`; `1` and `.3` by default.
- **Overshoot:** back and elastic go past the end value as a `cubic` with `y` beyond 1 does. A moving point carries on along the path's direction, and a property's range still clamps the value.

**Springs.** `spring(bounce)` moves like a spring released from rest that settles on the end value.
- **Bounce** lies between -1 and 1. At 0 the spring settles as fast as it can without overshooting. Above 0 it overshoots and rings: `spring(.5)` overshoots by about 16%. Below 0 it creeps in more slowly.
- **Perceived duration:** `spring(bounce,perceived)` sets how long the move feels, as a time such as `10f` or `.5s`; 0.5 s by default. It is one period of the spring without damping.
- **Timing:** in a key span the spring is scaled to fit the span. A behaviour that gives no duration lasts until the spring comes to rest, within 1/2000 of the change: `pop spring(.4,10f)`.
- **Exact:** the rest time is solved exactly, and the end meets the key.

**Steps.** `steps(n)` moves in `n` equal steps, jumping at the end of each, and reaches the end value at the next key. `steps(n,start)` jumps at the start of each. At the key itself a property is always that key's value. `hold` is `steps(1)`.

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
- **Stagger.** With `stagger` above 0, the parts counted by `by` take turns: each part sees the animator, its selectors included, as it was its wait earlier, so every part makes the whole motion with its own curve. A part waits `stagger` frames per turn. `order` sets the turns: `first` in reading order, `last` the reverse, `center` outward from the middle (the two middle parts of an even count go together, half a turn in), `edges` inward from both ends, and `random` a shuffle picked by `seed`. A glyph in no part, such as a space under `word`, sees the animator as it is.

| Animator | Key holds | Properties | Settings |
|---|---|---|---|
| `animator` | its items | `position`, `anchor`, `scale`, `rotate`, `opacity`, `fill`, `stroke`, `stroke-width`, `tracking`, `stagger`, `seed` | `by`: `char`, `char-no-space`, `word`, `line`; `order`: `first`, `last`, `center`, `edges`, `random` |

| Selector | Key holds | Properties | Settings |
|---|---|---|---|
| `range` | `by` | `start`, `end`, `offset`, `amount`, `smooth` | `shape`: `square`, `ramp-up`, `ramp-down`, `triangle`, `round`, `smooth`; `mode`: `add`, `subtract`, `intersect`, `min`, `max`, `difference` |

## Text motion

A text layer's `in` can move the parts of the text instead of the whole layer, one after another:

```json
"in":"fade-up word 30px 10f out-back stagger 3f"
"in":"pop char spring(.5) stagger 2f center"
```

- A part word, `char`, `char-no-space`, `word` or `line`, makes each part enter by the preset about its own anchor.
- `stagger <time>` is how long each part waits after the one before it, and an order word (`first`, `last`, `center`, `edges`, `random`) is who goes first. Both need a part word.
- The preset may move `position` (`x`, `y`), `scale`, `rotate` and `opacity`, which become the same values of a text animator. A preset that moves anything else is an error on a text's parts.
- It becomes an animator ahead of the layer's own `animate`: the values go from the preset's start to rest over the behaviour's frames and curve, a `range` by the same parts chooses them all, and the parts take turns as `stagger` says. The file keeps the one line.

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
- **A missing font or glyph** is an error, never a silent fallback. Only an import substitutes a font it does not have, by writing another face's name into the scene and reporting it ([D40](decisions.md#d40-missing-fonts-are-substituted-at-import)).

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
