Status: Draft · 2026-11-13

# Scene format

A nakkash scene is a small JSON file that describes a video: its size, length and layers, and what each layer does over time. It is built to be cheap for an agent to write. Defaults cover most values, and short strings say a lot in a few tokens.

## An example

```json
{"nakkash":1,"size":"1080p","fps":30,"dur":"5s","bg":"#0B0D12",
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

The syntax shown here is illustrative, and `repeat`, `layout`, `data`, `item` and `fx` are designed but not built yet. What is built today is the canvas, `text`, `rect`, `image` and `video` layers, `fill`, `at`, `in`, `out`, `anim` and `presets`; the sections below say what is still planned.

## The canvas

The top of the file sets the canvas:

- **`size`:** a preset such as `1080p`, `4k` or `square`, or exact pixels such as `1920x1080`.
- **`fps`:** frames per second.
- **`dur`:** the length of the clip.
- **`bg`:** the background colour.
- **`shutter`:** what layers with `mblur` blur over, as `"angle phase samples"` ([D57](decisions.md#d57-motion-blur-samples-a-layers-motion-over-the-shutter)).
  - The angle is how long the shutter stays open, in degrees of a frame, 0 to 720. 360 is a whole frame, and 0 turns motion blur off.
  - The phase is where it opens, in degrees from the frame's time, -360 to 360.
  - The samples are how many moments a frame takes, 2 to 64.
  - The default is `"180 -90 16"`, and the numbers left out keep theirs.
  - A composition under `comps` has its own shutter, the default unless it sets one.

**Units.** Time is counted in frames (`12f`), and seconds are accepted (`0.4s`). Colours are hex. Numbers are kept to 4 significant digits.

## Layers

Layers are stacked from bottom to top, in the order they are listed.

Every layer has an `id`: lowercase letters, digits, `-` and `_`, starting with a letter, and unique in its list. `scene` is reserved, because it names the scene itself in an address. A layer anywhere in a scene is addressed by the ids down to it, joined by `/`: `card/title` is the layer `title` inside the comp that `card` shows ([D43](decisions.md#d43-one-layer-address-and-edits-per-instance)).

| Layer | What it is |
|---|---|
| `text` | Text, with full shaping for any script |
| `rect` | A rectangle the size of its box, with an optional corner radius (`"300x100 r12"`) |
| `shapes` | Shape items: groups, rectangles, ellipses, stars and polygons, and free paths, drawn by fills and strokes (see [Shapes](#shapes-masks-effects-and-compositions)) |
| `svg` | An SVG file as shapes |
| `image` | A PNG or JPEG file, or vector footage (PDF, an `.ai` file that is PDF inside, or EPS), stretched over the layer's `box` (see [Images](#images)) |
| `video` | A video clip played in the layer's own time and stretched over its `box` (see [Videos](#videos)) |
| `solid` | A flat colour |
| `comp` | A composition from `comps`, drawn in the layer's box at the layer's own time; `overrides` changes its layers for this layer alone, and `collapse` draws them straight into the canvas the layer sits in |
| `pivot` | An invisible layer (`"pivot":true`) that only carries a position, rotation and scale. Other layers can use it as a parent. |
| filter layer | A layer with only an `fx` list. It applies the effects to everything below it in its composition (see `adjust`). |
| `camera` | The viewpoint for 2.5D layers |
| `audio` | A sound file played in the layer's own time; it draws nothing (see [Audio](#audio)) |

Lights and 3D models come in phase 2. See [Features](features.md).

Every layer can also have:

- **position and transform:** `at`, `anchor`, `scale`, `rotate`, `opacity`, plus `z`, `rotate-x`, `rotate-y` and `orient` for depth on a 3D layer (see [Depth](#depth))
- **`parent`:** another layer to follow
- **`blend`:** how the layer blends with what is below it in its composition: `normal` (the default), `multiply`, `screen`, `overlay`, `darken`, `lighten`, `color-dodge`, `color-burn`, `hard-light`, `soft-light`, `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity` or `add`. A composition is drawn on its own, so its layers blend only with each other, unless it is collapsed
- **`mask`:** outlines that cut what the layer draws, with feather and expansion, before its `fx` ([D55](decisions.md#d55-masks-are-items-that-cut-a-layers-content-before-its-effects); see [Shapes, masks, effects and compositions](#shapes-masks-effects-and-compositions))
- **`matte`:** another layer of the same list whose drawing is a stencil: `"matte":"reveal alpha"` shows the layer where `reveal` is opaque. The modes are `alpha` (the default), `alpha-inverted`, `luma` and `luma-inverted`; luma reads transparency as black. The matte layer is drawn as it would be at that moment, with its transform, parents, opacity, effects, span and its own matte. Whether it is also drawn in the frame is up to its `hidden`, and a hidden layer still mattes
- **`fx`:** a list of effects
- **`adjust`:** `true` applies the layer's `fx` to everything below it in its composition instead of to its own content, which is not drawn but says where they apply, by its alpha. A filter layer, with no content, applies them everywhere. The change is mixed in by the layer's opacity and matte; points and lengths of its effects are in the layer's own coordinates ([D49](decisions.md#d49-effects-that-read-pixels-run-on-a-canvas-of-their-own))
- **`collapse`:** on a `comp` layer, `true` draws the composition's layers straight into the canvas the layer sits in, under the layer's transform and opacity: they are not clipped to the composition's canvas, its background is not drawn, and their blend modes reach what is below unless the layer has effects ([D54](decisions.md#d54-a-collapsed-composition-draws-into-the-canvas-it-sits-in)). The layer's effects then run over that canvas, so their points and lengths are in its coordinates, not the layer's
- **`hidden`:** `true` leaves it out of the frame but keeps it in the scene; its children still move with it
- **`mblur`:** `true` blurs the layer along its motion. It is drawn at moments evenly spread over its composition's open shutter, each moment its whole drawing with content, effects and parents. Those drawings are averaged and then blend, fade and matte as one. Whether it draws is decided at the frame, so it doesn't fade at the ends of its span. A layer whose corners move more px over the shutter than it has samples takes one sample per px, up to 8 times as many. A `comp` layer, or one with effects, is drawn once at the frame and moved to each moment, up to 2 times as many; its composition's layers blur by that composition's own `shutter`. A layer with `particles`, which fly on with time inside the effect, is drawn at each moment, effects and all, up to 2 times as many. Adjusting layers, and layers whose effects read another layer, are drawn unblurred On a `pivot`, which draws nothing, it blurs every layer the pivot carries, at any depth.
- **`3d`:** `true` makes the layer a plane in its composition's 3D space, seen through the composition's camera (see [Depth](#depth))
- in and out points, and a time offset

## Depth

A layer with `"3d":true` is a flat plane that moves and turns in its composition's 3D space, and a `camera` layer is the viewpoint ([D58](decisions.md#d58-25d-layers-are-planes-projected-through-a-camera)):

```json
{"id":"cam", "camera":true, "z":-1800, "target":[960,540]},
{"id":"card", "rect":"600x400", "3d":true, "anim":{"rotate-y":"-90 -> 0 @0-0.6s out-cubic"}}
```

- **Space.** X runs right, Y down and Z away from the viewer, in px. A turn about Z is clockwise on screen, as `rotate` is; turns about X and Y follow the same right-hand rule, so a positive `rotate-y` brings the right edge closer.
- **The transform.** The anchor moves to the origin; then the layer scales, turns about Z (`rotate`), Y and X, takes its `orient`, and moves to its position. As a product: `T(position, z) · O · Rx · Ry · Rz · S · T(−anchor)`, after its parent's.
- **Depth rows.** `z`, `anchor-z`, `scale-z`, `rotate-x`, `rotate-y` and `orient` are for 3D layers and cameras only, and `zoom`, `target` and `target-z` for cameras only: on another layer they are an error.
- **Parents.** A 3D layer under a flat parent moves with the parent's transform in the plane at depth 0. A flat layer under a 3D parent moves with the parent seen straight on, without its depth.
- **The camera.** The last `camera` layer of the list that shows at a frame, and is not `hidden`, is the composition's camera. It draws nothing, is always 3D, and can be a parent.
  - It looks along its own Z axis from its position. A camera that sets or animates `target` turns to look at that point first, then takes its `orient` and rotations.
  - `zoom` is the distance at which a plane facing it shows at 100 %. A camera's scale and anchor do not apply.
  - Without a `z`, a camera sits at minus its zoom, so it sees the plane at depth 0 as a flat layer would be drawn.
- **Order.** Layers draw in list order, except that each run of adjacent 3D layers draws back to front. Where the boxes of two show over each other, one wholly behind the other's plane draws first, so a label laid 1 px in front of a card stays in front however the card turns; two in one plane keep list order. Otherwise, and to break a cycle, the one whose box's centre is deeper in front of the camera draws first, then the first in the list. A flat layer that draws ends a run; one that draws nothing, such as a pivot, a camera or an audio layer, does not. Planes that cross each other are drawn whole, in that order, not cut where they meet.
- **Compositions.** A `comp` layer draws its composition through that composition's own camera. A collapsed one's 3D layers join the outer composition's space under the layer's transform, and are seen through the outer camera, sorted among themselves.
- **The default camera.** A composition without a camera that shows is seen through one centred on its canvas, at `z` minus its zoom, with a zoom of `width · 50/36`: a 50 mm lens on 36 mm film, a 39.6° horizontal view.

## Properties

Every animatable value of a layer is a **property**. They are listed once, in `crates/nakkash-format/src/property.rs`; this table mirrors that list, and a test checks that the names match. Each property holds a typed value: a number, a point (`[x, y]`) or a colour ([D31](decisions.md#d31-typed-values-and-value-space-keys)).

| Property | Type | Rests at | Components | Meaning |
|---|---|---|---|---|
| `position` | point | where `at` puts it | `x`, `y` | Where the anchor sits in the parent, in px. |
| `anchor` | point | the centre of the layer's box | `anchor-x`, `anchor-y` | The point of the layer that sits at `position`, and that it scales and turns about, in px of its own box. |
| `scale` | point | `[1, 1]` | `scale-x`, `scale-y` | Size factor on each axis. One number scales both; negative values mirror. |
| `rotate` | number | 0 | | Rotation in degrees, clockwise. |
| `opacity` | number | 1 | | How opaque the layer is, 0 (invisible) to 1 (solid). |
| `volume` | number | 1 | | The gain of the sound an `audio` or `comp` layer plays: 1 as recorded, 0 silent, up to 4 (+12 dB). |
| `z` | number | 0; a camera's minus its zoom | | The depth of `position` on a 3D layer or camera, in px, away from the viewer as it grows. |
| `anchor-z` | number | 0 | | The depth of `anchor` on a 3D layer. |
| `scale-z` | number | 1 | | The size factor along depth of a 3D layer, which its children's depths feel. |
| `rotate-x` | number | 0 | | The turn of a 3D layer about its X axis, in degrees. |
| `rotate-y` | number | 0 | | The turn of a 3D layer about its Y axis, in degrees. |
| `orient` | orientation | `[0, 0, 0]` | `orient-x`, `orient-y`, `orient-z` | How a 3D layer or camera is turned, three angles in degrees applied after its rotations. Its keys turn the shortest way between them. |
| `zoom` | number | `width · 50/36` of the box `at` resolves in | | A camera's distance in px at which a plane facing it shows at 100 %; the default is a 50 mm lens on 36 mm film. |
| `target` | point | where `at` puts it | `target-x`, `target-y` | The point a camera turns to look at, in its parent's px, when the camera sets or animates it. |
| `target-z` | number | 0 | | The depth of a camera's `target`. |
| `remap` | number | its own time | | The time a `comp` or video layer shows of its content, in seconds, at each moment of its own time (see **Time remapping** below). |

**Resting values.** `at` sets where `position` rests, read in the parent's box. A layer may instead write `position` itself, an exact point in the parent's own coordinates, as imported scenes do; it cannot have both. Any other property rests where the layer sets it, by its name as a layer key, or at the value above:

```json
{"id":"diamond","rect":"500x500","at":"526 1254","rotate":45,"scale":0.8,"anchor":[242,109]}
```

**Components.** A track animates a whole property or one component of it: `"x"` moves only across, and `"scale"` animates both axes of scale. A property is animated whole or by its components, not both.

**Parenting and time.** A layer can move with another (`"parent":"card"`): its position, rotation and scale apply on top of the parent's, while opacity stays its own. Its `at` is then read in the parent's box, so `"at":"center center"` is the middle of the parent, and `nakkash at` prints where its anchor lands on the canvas. `"span":"1s-16s"` sets when a layer shows, on the scene's timeline (parenting moves a layer, not its time). `"offset":"1s"` says where the layer's own time starts: its keys and its entrance are in its own time, so moving a layer later is one edit. An offset may be negative, so a layer can start partway through its own time. `"stretch":2` plays its own time at half speed, and a negative stretch runs it backwards from `offset`. Speeds in `speed(...)` are per second of the layer's own time, so a stretch slows them too.

**Time remapping.** A `comp` or video layer may say which moment of its content it shows at each moment of its own time: `"remap"` is that moment in seconds of the composition or clip ([D62](decisions.md#d62-a-layer-may-remap-the-time-of-its-content)). Its keys are in the layer's own time, like any other track, so `"anim":{"remap":"0 -> 2 @0-4s"}` plays two seconds of the content over four, `"remap":1.5` holds one moment, and keys that fall back run it backwards. Only what the layer shows follows `remap`: its own transform and effects keep its own time. A moment before the content's start or from its end on shows nothing, as when it plays on its own. A `comp` layer's sound plays the remapped moment at the rate the remap moves, as a `stretch` would. A layer that neither sets nor animates `remap` shows its content at its own time. Only `comp` and video layers take a `remap`.

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

## Shapes, masks, effects and compositions

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
- **Merges.** A `merge` takes every outline listed before it in its group, the outlines inside earlier groups included, seen through those groups' transforms, and leaves one outline in their place ([D65](decisions.md#d65-a-merge-replaces-the-outlines-above-it-by-one)). The paints listed before it draw nothing of what it takes; the paints after it, and the modifiers after it, see the one outline it leaves.
- **Repeaters.** A `repeater` draws everything listed before it in its group again, as copies: as if those items were one group per copy, each under the copy's transform and opacity ([D67](decisions.md#d67-a-repeater-copies-the-items-above-it-as-groups)). The paints before it draw every copy, faded by the copy's opacity; the trims and paints after it reach the outlines of every copy and draw them as they draw any other, unfaded. A later repeater in the same group copies an earlier one with its copies.
- **Expansions.** An `expand` moves every outline listed before it in its group out, the outlines inside earlier groups included, each in its own space, as a trim cuts them: every fill and stroke that draws the outline draws the moved one, wherever it sits in the list. It acts in list order with the other modifiers of its group, after those of the groups inside it.
- **Space.** A paint fills and strokes in its own group's space, so a stroke's width scales with the groups around it, not with the groups it reaches into.
- **Fill rule.** All the outlines one fill covers count as one path for its `rule`, so `evenodd` cuts holes where they overlap.
- **Groups.** A group has a layer's transform (`position`, `anchor`, `scale`, `rotate`, `opacity`), resting at the origin. Its opacity multiplies the paints inside it.
- **Box.** A shape layer's own coordinates start at its anchor, `[0, 0]`; `check` measures the outlines it draws, widened by half of each stroke.
- **Ids and `hidden`.** An item may have an `id`, unique within its layer; `"hidden":true` leaves it out of the frame.

Each kind lists its properties in `crates/nakkash-format/src/item.rs`; this table mirrors that list, and a test checks it.

| Item | Key holds | Properties | Settings |
|---|---|---|---|
| `group` | its items | `position`, `anchor`, `scale`, `rotate`, `opacity` | |
| `rect` | `size` | `size`, `position`, `round` | |
| `ellipse` | `size` | `size`, `position` | |
| `star` | `points` | `points`, `position`, `rotate`, `outer-radius`, `inner-radius`, `outer-roundness`, `inner-roundness` | `type`: `star`, `polygon`; `direction`: `clockwise`, `counterclockwise` |
| `path` | `path` | `path` | |
| `fill` | `color` | `color`, `opacity` | `rule`: `nonzero`, `evenodd` |
| `stroke` | `color` | `color`, `opacity`, `width`, `miter` | `cap`: `butt`, `round`, `square`; `join`: `miter`, `round`, `bevel` |
| `trim` | `end` | `end`, `start`, `offset` | `mode`: `each`, `joined` |
| `merge` | `mode` | | |
| `round-corners` | `radius` | `radius` | |
| `repeater` | `copies` | `copies`, `offset`, `position`, `anchor`, `scale`, `rotate`, `start-opacity`, `end-opacity` | `composite`: `below`, `above` |
| `expand` | `amount` | `amount`, `miter`, `copies`, `copy-offset` | `join`: `miter`, `round`, `bevel` |

`size` splits into `width` and `height`, and `position` into `x` and `y`, for tracks. A rectangle's `round` is capped at half its shorter side. A `path` is SVG path data with absolute `M`, `L`, `C` and `Z` (`"M0,0 C40,0 80,40 80,80 Z"`), and, as in SVG, `""` is the path with no vertex, which draws nothing; a track on it uses a key list, and every key keeps the same number of vertices and the same closure, so the outline moves vertex by vertex.

A `star` puts its corners at equal angles around `position`, the first straight up from it, turned clockwise by `rotate` degrees; its outline runs from there clockwise, or the other way round with `"direction":"counterclockwise"`. A `star` (the default `type`) alternates `points` points `outer-radius` px out with as many inner corners `inner-radius` px out; a `polygon` has `points` corners `outer-radius` px out, and no inner ones. `points` runs from 0 to 100, and only its whole part counts: 4.9 draws four. A roundness gives each corner of its circle two handles along that circle, one each way, as long as the roundness times a quarter of the arc between two neighbouring points, `2π r / (4 points)`; 0 is sharp, with straight sides between sharp corners, and a negative roundness turns the handles back so the sides loop. `{"star":5,"outer-radius":60,"inner-radius":25}` is a five-pointed star, `{"star":6,"type":"polygon","outer-radius":40,"outer-roundness":0.3}` a softened hexagon.

A `trim` keeps the stretch from `start` to `end`, shares of the length from 0 to 1 (`{"trim":0.5}` keeps the first half), swapped if `start` is after `end`. `offset` moves the stretch along in degrees, 360 once around; a stretch that runs past the end goes on around a closed outline as one line and continues an open one as a second piece. `each` cuts every outline the same way, `joined` cuts them as one line in order. Equal `start` and `end` draw nothing, not even a cap. A cut closed outline is open: a stroke ends in its caps, and a fill closes it with a straight line. Lengths are arc lengths in the space each outline sits in.

A `merge` combines the outlines it takes in list order. `{"merge":"merge"}` lays them one after another as they are, so the fill rule of a paint after it counts across them as before. The other modes combine the areas they cover, each outline read by the `nonzero` rule: the first outline's area with the second's, that with the third's, and so on. `add` keeps what either covers, `subtract` what the first covers and the next does not, `intersect` what both cover and `exclude` what one covers and the other does not, so `subtract` cuts every later outline out of the first. Their result is made of closed outlines that do not cross, outer ones clockwise and holes the other way, so it covers the same area under either `rule`; an open outline is closed by a straight line first, as a fill closes it. `merge` is the default. A trim before a merge cuts the outlines it takes; a trim after it cuts the outline it leaves, along its length from wherever its contours start.

`round-corners` rounds the sharp corners of every outline listed before it in its group, the outlines inside earlier groups included, each in its own space, as a trim cuts them: every paint that draws an outline draws it rounded ([D66](decisions.md#d66-round-corners-cut-sharp-corners-back-along-their-sides)). A sharp corner is a vertex with no handle on either side; it is cut back along each of its sides by `radius` px, or by half the way to the vertex at the other end of that side when that is shorter, and the two cut points are joined by a cubic whose handles reach 0.5519 of the way back towards the corner, a quarter circle where the sides meet square. Vertices with a handle, and the two ends of an open outline, stay as they are. `radius` defaults to 10.

A `repeater` draws `copies` copies, the first in place, and steps `[100, 0]` unless its `position` says otherwise. Copy `k` stands `t = k + offset` steps along: moved by `t` times `position`, turned clockwise by `t` times `rotate` degrees and scaled by `scale` to the power `t`, the turn and scale about `anchor`, in px of the group. The steps add up but do not compound: copy 2 stands two `position`s from the first however the copies turn, so a turn with no move fans copies about the anchor and a move with no turn lines them up. A negative scale mirrors every other copy. Copy `k` of `n` takes the opacity `start-opacity + (end-opacity − start-opacity) · k / n`, so the last stops a step short of `end-opacity`; the offset moves the copies but not their fade. A fractional count draws one copy more, its opacity times the fraction, so an animated count fades each copy in. With `composite` `below` each copy draws under the one before and the first is on top; `above` puts each over the one before. `{"repeater":12,"position":[0,0],"anchor":[0,80],"rotate":30}` rings what is above it twelve times around a point 80 px below it.

An `expand` moves every side of an outline `amount` px to its left, which is outward for an outline that runs clockwise, as `rect`, `ellipse` and `star` do, and inward with a negative amount ([D68](decisions.md#d68-an-expansion-moves-outlines-out-by-an-amount)). Where the outline turns away from the side it moves to, the moved sides part and `join` closes the gap: `miter` extends them until they meet, unless that point is more than `miter` amounts from the corner, then it bevels; `round` draws an arc about the corner; `bevel` a straight line. Where the outline turns towards that side, the moved sides cross and both are cut where they cross. An open outline is moved along both its sides, out and back, its ends joined the same way, and becomes one closed outline around it. Each outline becomes `copies` outlines, counting the whole part (0 leaves none), copy `k` from 1 moved `amount · (k + copy-offset)`. Lengths are in the space each outline sits in, as a trim's are. `{"rect":[300,100],"round":12},{"expand":20}` is a 340 by 140 rectangle rounded by 32.

**Masks.** `mask` is a list in the same item form, of the one kind `mask`, that cuts what the layer draws before its effects run ([D55](decisions.md#d55-masks-are-items-that-cut-a-layers-content-before-its-effects)):

| Item | Key holds | Properties | Settings |
|---|---|---|---|
| `mask` | `path` | `path`, `feather`, `opacity`, `expand` | `mode`: `add`, `subtract`, `intersect`, `lighten`, `darken`, `difference`, `none`; `keep`: `inside`, `outside` |

- **The outline.** `path` is in the layer's own coordinates, written as a shape's `path` is. It closes when it is open, and keys of one track keep their vertex count.
- **One mask's coverage.** It covers what is inside its outline, or what is outside it with `"keep":"outside"`, times its `opacity`.
  - `expand` moves the edge out by that many px, or in when negative.
  - `feather` fades the edge over about that many px, half inside and half outside. It is a Gaussian whose standard deviation is 0.3 of it, and one number fades both axes.
- **Combining.** The masks combine in order, each with the coverage of the ones before it, `a`, as its `mode` says:
  - `add` gives `a + m − a·m`;
  - `subtract` gives `a·(1 − m)`;
  - `intersect` gives `a·m`;
  - `lighten` and `darken` give the larger and the smaller;
  - `difference` gives `|a − m|`;
  - `none` is skipped.

  Before the first mask that is not `none`, `a` is 0 when that mask adds, lightens or differs, and 1 otherwise. A layer whose masks are all `none` is not cut.
- **Order of drawing.** The coverage cuts the content first. Then the effects run, so a blur spreads past the mask's edge, and then the matte and opacity apply. A `transform` effect moves the cut content with its mask.
- **Outlines to follow.** An effect that draws along a path, `audio-spectrum` and `audio-waveform`, may follow a mask's outline: its `along` names the mask by `id`, which must be one of the same layer's masks.
- **Filter layers.** On a filter or adjusting layer, the masks say where its effects apply. A filter layer's own coordinates have their origin where its anchor is: the composition's centre, unless it is moved.

```json
{"id":"photo","image":"photo.jpg","mask":[{"mask":"M0,0 L800,0 L800,600 L0,600 Z","feather":40},{"mask":"M300,200 C400,200 400,400 300,400 Z","mode":"subtract"}]}
```

**Effects.** `fx` is a list in the same item form, applied in order to what the layer draws:

| Effect | Key holds | Properties | Settings |
|---|---|---|---|
| `fill` | `color` | `color`, `opacity` | `inverted`: `off`, `on` |
| `solid-composite` | `color` | `source-opacity`, `color`, `opacity` | `mode`: `normal`, `add`, `multiply`, `screen`, `overlay`, `soft-light`, `hard-light`, `color-dodge`, `color-burn`, `darken`, `lighten`, `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity` |
| `transform` | `scale` | `position`, `anchor`, `scale`, `rotate`, `opacity` | |
| `tint` | `amount` | `black`, `white`, `amount` | |
| `hue-saturation` | `hue` | `hue`, `saturation`, `lightness`, `colorize-hue`, `colorize-saturation`, `colorize-lightness` | `mode`: `adjust`, `colorize` |
| `color-cycle` | `phase-from` | `red`, `green`, `blue`, `alpha`, `shift`, `repetitions`, `blend` | `add-from`: `intensity`, `red`, `green`, `blue`, `hue`, `lightness`, `saturation`, `value`, `alpha`, `zero`; `add-mode`: `wrap`, `clamp`, `average`, `screen`; `modify`: `all`, `red`, `green`, `blue`, `red-green`, `green-blue`, `red-blue`, `hue`, `lightness`, `saturation`, `hue-lightness`, `lightness-saturation`, `hue-saturation`, `none`; `interpolate`: `on`, `off`; `modify-alpha`: `on`, `off` |
| `exposure` | `exposure` | `exposure`, `offset`, `gamma` | `space`: `linear`, `gamma` |
| `levels` | `gamma` | `input-black`, `input-white`, `gamma`, `output-black`, `output-white` | `channel`: `rgb`, `red`, `green`, `blue`, `alpha` |
| `particles` | `rate` | `rate`, `longevity`, `position`, `radius-x`, `radius-y`, `velocity`, `gravity`, `resistance`, `direction`, `extra`, `birth-size`, `death-size`, `size-variation`, `max-opacity`, `birth-color`, `death-color`, `seed` | `animation`: `explosive`, `direction`; `shape`: `faded`, `disc`, `bubble`; `opacity-map`: `fade-out`, `constant`, `fade-out-sharp`, `fade-in`, `fade-in-out`; `transfer`: `over`, `screen`, `add` |
| `displacement-map` | `horizontal` | `horizontal`, `vertical` | `horizontal-from`: `red`, `green`, `blue`, `alpha`, `luminance`, `hue`, `lightness`, `saturation`, `full`, `half`, `off`; `vertical-from`: `green`, `red`, `blue`, `alpha`, `luminance`, `hue`, `lightness`, `saturation`, `full`, `half`, `off`; `behaviour`: `center`, `stretch`, `tile`; `edges`: `clear`, `wrap` |
| `set-matte` | `channel` | | `inverted`: `off`, `on`; `behaviour`: `stretch`, `center`; `composite`: `on`, `off`; `premultiply`: `on`, `off` |
| `shift-channels` | `alpha-from` | | `red-from`: `red`, `alpha`, `green`, `blue`, `luminance`, `hue`, `lightness`, `saturation`, `full`, `off`; `green-from`: `green`, `alpha`, `red`, `blue`, `luminance`, `hue`, `lightness`, `saturation`, `full`, `off`; `blue-from`: `blue`, `alpha`, `red`, `green`, `luminance`, `hue`, `lightness`, `saturation`, `full`, `off` |
| `audio-spectrum` | `height` | `start`, `end`, `height`, `duration`, `offset`, `thickness`, `softness`, `inside-color`, `outside-color`, `from`, `to`, `bands`, `hue` | `display`: `digital`, `lines`, `dots`; `side`: `both`, `a`, `b`; `path`: `line`, `circle`; `average`: `off`, `on`; `symmetry`: `on`, `off`; `composite`: `off`, `on` |
| `audio-waveform` | `height` | `start`, `end`, `height`, `duration`, `offset`, `thickness`, `softness`, `inside-color`, `outside-color`, `samples` | `display`: `digital`, `lines`, `dots`; `channel`: `mono`, `left`, `right`; `path`: `line`, `circle`; `composite`: `off`, `on` |
| `motion-tile` | `tile-width` | `center`, `tile-width`, `tile-height`, `output-width`, `output-height`, `phase` | `edges`: `repeat`, `mirror`; `shift`: `vertical`, `horizontal` |
| `wrap` | `to` | `to`, `blend` | |
| `turbulent-displace` | `amount` | `amount`, `size`, `offset`, `complexity`, `evolution`, `seed` | `displacement`: `turbulent`, `turbulent-smoother`, `bulge`, `bulge-smoother`, `twist`, `twist-smoother`, `horizontal`, `vertical`, `cross`; `pinning`: `all`, `left-right`, `top-bottom`, `none` |
| `mirror` | `center` | `center`, `angle` | |
| `polar-coordinates` | `interpolation` | `interpolation` | `conversion`: `rect-to-polar`, `polar-to-rect` |
| `lens-distortion` | `field-of-view` | `field-of-view`, `center` | `distortion`: `add`, `remove`; `field`: `horizontal`, `vertical`, `diagonal`; `grow`: `off`, `2x`, `4x` |
| `bezier-warp` | `top-left` | `top-left`, `top-handle-left`, `top-handle-right`, `top-right`, `right-handle-top`, `right-handle-bottom`, `bottom-right`, `bottom-handle-right`, `bottom-handle-left`, `bottom-left`, `left-handle-bottom`, `left-handle-top` | |
| `repeat-tile` | `tiling` | `right`, `left`, `down`, `up` | |
| `ramp` | `blend` | `start`, `start-color`, `end`, `end-color`, `blend` | `shape`: `linear`, `radial` |
| `four-color` | `opacity` | `point-1`, `color-1`, `point-2`, `color-2`, `point-3`, `color-3`, `point-4`, `color-4`, `blend`, `opacity` | `mode`: `replace`, `over`, `add`, `multiply`, `screen`, `overlay`, `soft-light`, `hard-light`, `color-dodge`, `color-burn`, `darken`, `lighten`, `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity`, `stencil-alpha` |
| `beam` | `time` | `start`, `end`, `length`, `time`, `start-thickness`, `end-thickness`, `softness`, `inside-color`, `outside-color` | `composite`: `off`, `on` |
| `grid` | `border` | `anchor`, `corner`, `width`, `height`, `border`, `color`, `opacity` | `size`: `corner`, `width`, `width-height`; `draw`: `lines`, `cells`; `mode`: `replace`, `over`, `add`, `multiply`, `screen`, `overlay`, `soft-light`, `hard-light`, `color-dodge`, `color-burn`, `darken`, `lighten`, `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity`, `stencil-alpha` |
| `venetian-blinds` | `completion` | `completion`, `direction`, `width`, `feather` | |
| `linear-wipe` | `completion` | `completion`, `angle`, `feather` | |
| `iris-wipe` | `outer-radius` | `center`, `points`, `outer-radius`, `inner-radius`, `rotation`, `feather` | `inner`: `off`, `on` |
| `fractal-noise` | `evolution` | `contrast`, `brightness`, `rotation`, `scale`, `scale-width`, `scale-height`, `offset`, `complexity`, `sub-influence`, `sub-scaling`, `sub-rotation`, `sub-offset`, `evolution`, `seed`, `opacity` | `fractal`: `basic`, `turbulent-smooth`, `turbulent-basic`, `turbulent-sharp`, `smeary`, `swirly`; `noise`: `spline`, `soft-linear`, `linear`, `block`; `overflow`: `clip`, `soft-clamp`, `wrap-back`; `polarity`: `normal`, `inverted`; `mode`: `over`, `replace`, `add`, `multiply`, `screen`, `overlay`, `soft-light`, `hard-light`, `color-dodge`, `color-burn`, `darken`, `lighten`, `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity`, `stencil-alpha` |
| `gaussian-blur` | `blurriness` | `blurriness` | `dimensions`: `both`, `horizontal`, `vertical`; `edges`: `transparent`, `repeat` |
| `box-blur` | `radius` | `radius`, `iterations` | `dimensions`: `both`, `horizontal`, `vertical`; `edges`: `transparent`, `repeat` |
| `radial-blur` | `amount` | `amount`, `center` | `type`: `zoom`, `spin`; `stretch`: `behind`, `across`; `fade`: `none`, `linear` |
| `vector-blur` | `amount` | `amount`, `angle`, `ridge-smoothness`, `softness` | `type`: `natural`, `constant-length`, `perpendicular`, `direction-center`, `direction-fading`; `map-level`: `lightness`, `red`, `green`, `blue`, `alpha`, `luminance`, `hue`, `saturation` |
| `drop-shadow` | `distance` | `color`, `opacity`, `direction`, `distance`, `softness` | `layer`: `shown`, `hidden` |
| `glow` | `radius` | `threshold`, `radius`, `intensity` | `dimensions`: `both`, `horizontal`, `vertical`; `composite`: `with-original`, `glow-only`, `behind-original`; `operation`: `add`, `normal`, `multiply`, `dissolve`, `screen`, `overlay`, `soft-light`, `hard-light`, `darken`, `lighten`, `difference`, `hue`, `saturation`, `color`, `luminosity`, `color-dodge`, `color-burn`, `exclusion`, `stencil-alpha`, `stencil-luma`, `silhouette-alpha`, `silhouette-luma`, `premultiplied-over`, `alpha-add` |
| `sharpen` | `amount` | `amount` | |
| `find-edges` | `lines` | `blend` | |
| `black-white` | `tint` | `reds`, `yellows`, `greens`, `cyans`, `blues`, `magentas`, `tint`, `tint-color` | |
| `curves` | `rgb` | `rgb`, `red`, `green`, `blue`, `alpha` | |
| `invert` | `channel` | `blend` | |

A `transform` moves everything the layer draws, inside the layer, as a group's transform moves its items: `anchor` (px of the layer's own coordinates) goes to `position`, scaled and turned about it, and `opacity` multiplies the layer's. Both points rest at the origin, so `{"transform":1.5}` scales about the layer's origin; an import writes both. Several apply in order, each moving what the ones before it moved.

`particles` draws, in place of the layer, particles a producer sends out over the layer's own time: `rate` a second from an ellipse `radius-x` by `radius-y` px around `position`, each living `longevity` seconds, leaving at `velocity` px a second every way or towards `direction`, varied by `extra`, falling with `gravity` and slowed by `resistance`. Each grows from `birth-size` to `death-size` px and turns from `birth-color` to `death-color`. `rate` acts on births only: particle `i` is born when the running total of `rate` over the layer's own time, from 0, first passes `i` (at `i / rate` seconds while the rate holds), and then lives its `longevity` whatever the rate does, so a rate keyed down to 0 leaves a burst that lives out its time. The running total is worked out once from `rate`'s keys: exactly over held and straight spans, and over any other ease by cutting the span into 32 equal parts taken as straight, so a birth falls at the same moment whichever frame or shutter moment draws it. Its random draws come from `seed` and `i`, so every frame draws alone and the same, with at most 20,000 at once, the oldest left out. Every other property is read at the moment drawn and acts on every living particle.

`displacement-map` reads another layer, the one `map` names in the same list (its own drawing without `map`): that layer's drawing in its own coordinates, with its effects but not its transform, opacity or matte, as a project reads a layer parameter. A collapsed `comp` layer is read as its composition, uncollapsed: collapsing changes where the composition draws, not what an effect reads of it. Each pixel moves up to `horizontal` and `vertical` px by the map's levels at the same place, 0.5 moving nothing; the map's box lies centred on the layer's, stretched over it, or tiled, and a map layer may be hidden. A layer may not reach itself through the layers its effects read and its matte.

`set-matte` takes the layer's alpha from a level of another layer, the one `matte` names in the same list, read as `displacement-map` reads its map. Without `matte` it reads the layer's own drawing before any of its effects run: what the layer drew, cut by its masks, so a matte at the end of a chain of effects puts back the layer's own shape. The matte layer is laid on the layer by their effect boxes, each what its content fills (a rect, an image's or video's frame, a composition's canvas) or, for shapes and text, a box the size of the composition they sit in centred on their origin; a collapsed composition's box is the canvas it draws into. With `behaviour` `stretch` the matte's box is stretched over the layer's, with `center` its middle lies on the layer's at its own size; boxes of one size lie the same either way. `channel` picks the level, read from the matte's colour laid over black (`premultiply` `on`) or as it is (`off`): a colour, `alpha`, `luminance` (Rec. 601 weights), `hue` (a turn from red, 0 for greys), `lightness` (the mean of the largest and smallest colour), `saturation`, or `full` (1) and `off` (0). `inverted` turns it into its opposite. With `composite` `on` it multiplies the layer's own alpha; with `off` it replaces it inside the layer's box, over the layer's straight colour, black where the layer drew nothing.

`shift-channels` rebuilds each channel of the layer from a level of its straight colour and alpha as they were: `alpha-from` (which the kind's key holds), `red-from`, `green-from` and `blue-from` each name a channel, `luminance`, `hue`, `lightness` or `saturation` as `set-matte` reads them, or `full` (1) and `off` (0). Where the layer drew nothing its colour reads black, so only a `full` alpha draws there, inside the layer's box.

`color-cycle` sends each pixel round a cycle of colours. Its phase, 0 to 1, is a level of the pixel's straight colour (`phase-from`, which the kind's key holds): `intensity` (the mean of red, green and blue), a colour, `hue`, `lightness`, `saturation`, `value` (the largest colour), `alpha`, or `zero`. When `add` names a layer of the same list, a level of its drawing at the same place (`add-from`) joins the phase: summed and wrapped (`wrap`), summed and held at 1 (`clamp`), averaged, or screened. The phase runs `repetitions` times round the cycle, moved on by `shift` degrees of a turn. The cycle is four curves of the phase, `red`, `green`, `blue` and `alpha`, written as the `curves` effect writes its curves (`M x,y L x,y …`, at most 16 points in rising phase): each is joined straight from point to point and round from its last point back to its first, or held at each point until the next with `interpolate` `off`; an empty curve is the phase itself for a colour and 1 for alpha. The cycle's colour replaces the channels `modify` names, some of red, green and blue or of hue, lightness and saturation, or none; with `modify-alpha` `on` its alpha multiplies the pixel's, and `blend` keeps that much of the pixel as it was. Pixels the layer left empty stay empty.

`audio-spectrum` and `audio-waveform` draw another layer's sound in place of what their layer draws: the `audio` layer, or the `comp` layer whose composition's sound, `audio` names in the same list. They read a window `duration` ms long, centred `offset` ms after the layer's time, as the mix plays it there (volume and fades included), so every frame draws alone.

- **Where.** Bands or points stand along the line from `start` to `end`, in px of the layer, or round the circle about `start` through `end` with `path` `circle`. They reach up to `height` px out from it: to the left going from `start` to `end` on side `a`, to the right on `b`, both ways on `both`.
- **Along a mask.** `along` names one of the layer's masks by its `id`, and the bands or points stand along that mask's outline instead, from its first vertex, evenly by length: from end to end of an open outline, the first and last on its ends, and once round a closed one. Side `a` is then to the left going along the outline, so inward round a mask drawn counterclockwise. The outline is the mask's path as it is, whatever its feather and expansion, and the mask still cuts the layer's own drawing by its `mode` (`none` cuts nothing). A hidden mask is left out, and an effect that names it draws along its line.
- **Spectrum.** `bands` frequencies, evenly spaced from `from` to `to` Hz (`from` may be the higher), each as tall as the loudest part of the spectrum within half a band's spacing: a full-scale sine reaches `height`. `average` averages the window's quarters, overlapping by half, which steadies the bands. `hue` turns the colour's hue over the bands, in degrees, to the middle band and back with `symmetry` `on`.
- **Waveform.** `samples` points of the wave, evenly through the window, of both channels averaged or of one (`channel`).
- **Drawing.** `display` `digital` draws a band as a column of squares `thickness` px apart and the wave as a bar a point; `lines` as a solid bar and a line through the points; `dots` as a dot at the tip. Lines are `thickness` px thick, their edges faded over `softness` of it, coloured from `inside-color` along their middle to `outside-color` at their edges. The layer's own drawing is cleared under them unless `composite` is `on`.

The distortions read the layer's drawing at other points of its own coordinates. Those that tile, wrap or pin use the layer's box: a rect's, an image's or video's frame, a composition's canvas; layers without one (shapes, text) use the frame's. `motion-tile` repeats the box, shrunk to `tile-width` by `tile-height` of itself, around `center`, over `output-width` by `output-height` of the box about its middle; `mirror` flips every other tile and `phase` shifts every other column, or row with `shift` `horizontal`. `wrap` moves the box's middle to `to`, wrapping around it. `turbulent-displace` moves each pixel up to `amount` percent of `size` along a smooth noise of cells `size` px wide, in `complexity` octaves: both ways at once (`turbulent`), along one axis (`horizontal`, `vertical`), the same both ways (`cross`), out along the noise's slope (`bulge`) or round its hills (`twist`); pinned edges stay put, the movement fading in over a cell. `mirror` reflects the layer across the line through `center` that `angle` turns clockwise from upright: the side the angle faces (the right at 0°, the bottom at 90°) shows the other, and nothing shows outside the box. `polar-coordinates` with `rect-to-polar` wraps the box's rows into rings about its middle: the point at angle φ counterclockwise from straight up, `r` px from the middle, shows the layer φ/2π of the way across the box and √2·`r` px below its top, so the top row is the middle and the columns run round from straight up. `polar-to-rect` is its inverse, rings unrolled into rows. Each pixel goes `interpolation` of the way from where it is to where the conversion takes it, and nothing shows outside the box. `lens-distortion` bends the layer about `center` as a wide lens draws it: a lens that draws a ray at angle φ from its axis f·sin φ from the axis, where a pinhole draws it f·tan φ away, with its focal length f set so that `field-of-view` degrees span the box's width, height or diagonal (`field`) from one end to the other. `add` draws the layer as that lens sees it, drawn in towards the axis, with nothing where the lens would need rays at a right angle or wider; `remove` undoes it, stretching the layer out from the axis. A `field-of-view` of 0 bends nothing. It reads only the box and draws in it, or, with `grow` `2x` or `4x`, in the box grown to hold all it draws, but no more than twice or four times the box's width and height about its middle; the grown box is the one the effects after it see. `bezier-warp` lays the box on a patch bounded by four cubic curves: each side runs from corner to corner through its two handles. Every point is how far, in px, it moves from where it rests: the corners on the box's corners and the handles a third of the way along each side, so at rest the sides are straight and nothing moves. What lies `u` across and `v` down the box shows `u` across and `v` down the patch, which fills between its sides by blending the opposite ones (a Coons patch). Where the patch folds over itself the part nearer its bottom right shows, and its outline is smoothed over a pixel. It grows the box by as far as any point moves past it. `repeat-tile` repeats the box around itself as far as `right`, `left`, `down` and `up` px reach past its sides: every tile alike (`repeat`), every other column flipped across and every other row upside down so neighbours meet at their edges (`unfold`), every other row shifted by half a tile (`brick`), the rows and columns counted from the box, or no tiles at all (`none`), the box grown with nothing in it for the effects after it. It grows the layer's box, which the effects after it see, and the clip a composition's layer shows within; the content is cut to its own clip first. A collapsed composition's box is its canvas where it lands in the composition it draws into.

The generators draw where the layer is, by its alpha (a solid's box), worked out in the layer's own px so they move with it. `ramp` runs from `start-color` at `start` to `end-color` at `end`, along the line or around `start`, keeping `blend` of the layer's colour. `four-color` weighs its four colours by the inverse of the distance to their points to the power 2 × 100 / `blend`. `grid` draws lines `border` px wide around cells from `anchor`, or the cells. With `mode` `replace` they draw alone, with `over` over the layer, by `opacity`; with an operation of compositing inside an effect (below) they combine with the layer by it, the generator's colour at `opacity` over the layer's own, so `multiply` darkens the layer by the pattern and `stencil-alpha` keeps the layer where the pattern is. `fractal-noise` takes the same modes, `over` by default. `venetian-blinds` hides `completion` of every stripe `width` px wide, 0° being horizontal stripes. `linear-wipe` hides the layer behind a straight edge that crosses its box along `angle`, in degrees clockwise from up (90° runs left to right, so the left goes first), `completion` of the way; the edge fades over `feather` px, and its travel takes the feather in, so 0 shows the whole box and 1 hides it. `iris-wipe` cuts a hole about `center`: a polygon of `points` corners (rounded) `outer-radius` px from it, its first corner straight up turned `rotation` degrees clockwise, or with `inner` `on` a star whose every other corner is `inner-radius` px from it. Its edge fades inwards over `feather` px, so a radius of 0 hides nothing. `fractal-noise` is grey value noise in `complexity` octaves; a cell is 100 px at a `scale` of 1, each octave is `sub-scaling` the size and `sub-influence` the weight of the one before, and `evolution` moves through it, a turn per cell. It is the same kind of noise as a project's, not the same pixels.

The blurs run in the layer's own px, through its transform. `gaussian-blur` is a Gaussian whose standard deviation is 0.3 of `blurriness`, as three box passes; `box-blur` runs `iterations` boxes of `radius`. `dimensions` blurs both ways, along rows or along columns, and `edges` takes what lies beyond the frame as nothing or as the edge pixels repeated. `drop-shadow` draws the layer's alpha in `color` at `opacity`, `distance` px towards `direction` (degrees clockwise from up), blurred by `softness` as a blurriness, under the layer, or alone when `layer` is `hidden`. `glow` blurs the colours whose luma is past `threshold` by `radius` as a blurriness, times `intensity`; `composite` `with-original` lays that light over the layer by `operation`, `behind-original` lays the layer over the light by it, and `glow-only` draws the light alone. The operations are those of effects that combine their result with the layer (see compositing inside an effect, below). `sharpen` moves each pixel away from the mean of its four neighbours by `amount` percent of the difference. `find-edges` draws where the colours change as lines: each colour channel's slope across the 3×3 frame pixels around each pixel, by the Sobel operator scaled so a step from 0 to 1 gives 1, at most 1. With `light-on-dark` the edges are light on black, with `dark-on-light` dark on white; alpha stays, and `blend` of the layer as it was shows through. `radial-blur` makes each pixel the mean of the layer along the line through it that runs out from `center` (`zoom`) or round it (`spin`): over `amount` percent of its distance from the centre for a zoom, `amount` degrees for a spin. With `stretch` `behind` the stretch runs from the pixel back towards the centre, or clockwise, so the layer streaks outwards, or counterclockwise, and the other way for a negative amount; a zoom's stretch ends at the centre. With `across` it runs half either side. With `fade` `linear` the pixels of a stretch weigh from all at the pixel to none at its far end. The mean is taken over the whole stretch, not from samples, in time that does not grow with it; the centre goes through the layer's transform, and a spin's rings are circles of the frame. `vector-blur` smears the layer along the slope of one level (`map-level`) of `map`, another layer of the same list read as `displacement-map` reads it, or of its own drawing without `map`. The level is softened by `softness` and its slope by `ridge-smoothness`, each as a blurriness, so where the slope turns sharply, along a ridge or a valley, the streaks shorten rather than flip. Each pixel becomes the mean along a straight streak through it, turned `angle` degrees clockwise from the slope: `natural` both ways along the slope and `amount` px long at the map's steepest, shorter where it is gentler; `constant-length` always `amount` px; `perpendicular` across the slope, along the contours; `direction-center` behind the pixel only, down the slope, and `direction-fading` the same with the pixels weighing less the farther behind.

**Compositing inside an effect.** Some effects combine what they make with the layer's own pixels, and name how with an operation. Both sides are premultiplied colours. The blend modes (`normal`, `multiply`, `screen`, `overlay`, `soft-light`, `hard-light`, `color-dodge`, `color-burn`, `darken`, `lighten`, `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity`) blend the straight colours as the layer blend modes of those names do, then lay the blend over: where only one side covers a pixel that side shows, where both do their blend. `add` is the sum of both premultiplied colours and alphas, held to 1. The others do not blend colours: `replace` keeps the effect's result alone; `dissolve` shows the result, opaque, at a share of the pixels as large as its alpha, scattered by a fixed pattern of the pixel's place; `stencil-alpha` and `stencil-luma` keep the layer by the result's alpha or luma, `silhouette-alpha` and `silhouette-luma` cut it out by them; `premultiplied-over` lays the result's premultiplied colour over without holding it to its alpha first, so light past its alpha adds; `alpha-add` adds the alphas, the result covering what it can and the layer filling the rest, so two edges that share a pixel close it.

`solid-composite` lays the layer, at `source-opacity`, over a solid of `color` at `opacity` by `mode`, as compositing inside an effect does. The solid fills the layer's box (a rect's, an image's or video's frame, a composition's canvas; the frame for a layer without one), its edges covering the share of each pixel they do, so a clear layer shows the solid and a layer over an opaque solid comes out opaque within its box. The colour's own alpha changes nothing; `opacity` says how opaque the solid is.

The colour effects change each pixel on its own, on its straight colour, levels from 0 to 1. `tint` maps each colour's luma (Rec. 601) from `black` to `white` and mixes that in by `amount`. `hue-saturation` turns hues by `hue` degrees and moves saturation and lightness towards their ends by shares from -1 to 1; in `colorize`, every colour takes the `colorize-` hue and saturation at its own luma. `exposure` multiplies light by 2 to the `exposure`, adds `offset` and raises it to 1/`gamma`, on light decoded from sRGB in `linear` space. `levels` maps its channel from `input-black`–`input-white` to `output-black`–`output-white` through a power of 1/`gamma`. `black-white` turns each colour to grey: its lowest level, plus what its highest has over its middle times the share of its primary (`reds`, `greens`, `blues`), plus what its middle has over its lowest times the share of the secondary the two make (`yellows`, `cyans`, `magentas`); `tint` mixes in the hue and saturation of `tint-color` at that grey. Each `curves` property is a path of up to 16 points from input to output level in rising input, `"M 0,0 L 0.5,0.6 L 1,1"`, joined by a natural cubic spline and flat beyond its ends; a channel's own curve applies before `rgb`, and an empty path changes nothing. A curve animates as any path does, point by point, so its keys need the same number of points; keys on the same inputs then move from one curve's levels to the other's (the import puts keyed curves on shared inputs, see [the evidence](aep-evidence.md#animated-curves)). `invert` turns each level of its channel (`rgb`, `red`, `green`, `blue` or `alpha`) into 1 minus itself, keeping `blend` of the original.

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

## Slots

A scene, and any composition in `comps`, may declare `slots`: named values that layers follow ([D44](decisions.md#d44-slots-are-named-values-that-layers-follow)). A value written `"$id"` takes the value of slot `id`, so one change to a slot changes every value that follows it.

```json
{"nakkash":1,"size":"1080p","dur":"5s",
 "slots":{"accent":{"color":"#E63946","label":"Accent"},"title":{"text":"Launch"},"lift":40},
 "layers":[{"id":"plate","rect":"600x300 r24","fill":"$accent"},
           {"id":"headline","text":"$title","font":"Inter Variable 96 700"}]}
```

- **Types.** The value fixes the slot's type: a colour is a `color`, as a string or as `[r, g, b, a]`, a number a `number`, `[x, y]` a `point`. Words are written `{"text":"…"}`. Any type may be written out, `{"color":"#E63946"}`, with an optional `label` for people.
- **Scope.** A reference names a slot of the composition it is written in, then one of the scene. A composition never sees another composition's slots.
- **Dollars.** `$` followed by an id is a reference. A string starting with `$$` stands for itself with one `$` fewer, so a price is `"$$99"`. A `$` before anything that is not an id, as in `"$99"`, stays as written.
- **Checking.** A reference to no slot is an error at the value, `plate.fill`. A slot whose value does not fit where it is used is reported there, naming the slot: `plate.fill: … (the value of slot `$title`)`.
- References are resolved before anything else reads the scene, so drawing a scene with slots costs nothing more, and the file keeps the references as written.
- **Instances.** A `comp` layer gives its composition's slots values of its own in `overrides`, by `$id` next to the layer keys: `{"$title":"Sale","badge/icon":{"fill":"#E63946"}}`. The value replaces the slot's for every value of that instance's layers that follows it, and a layer key set by address wins over it. The value is read where the `comp` layer is written, so `"$title":"$promo"` passes on a slot of the outer scope, which is how a value reaches a composition drawn inside another. An id the composition does not declare, or a value that does not fit its slot, is an error at the entry, `card-2.overrides.$title`.

## Roles

A scene's `roles` say what its layers and slots do ([D50](decisions.md#d50-roles-name-what-a-masters-layers-do)): each role name, an id, names a layer by its address, at any depth, or a slot of the scene by `$id`.

```json
"roles":{"headline":"title","cta":"end/cta","accent":"$accent"}
```

A role that names no layer or slot is an error at it, `roles.headline`. An edit may target a role: `@headline text="Sale"` edits `title`, and `@accent=#2A9D8F` sets the slot. `nakkash roles` lists them with what each holds.

## Locks

A scene's `locks` list what a variant may not change ([D51](decisions.md#d51-locks-are-refused-in-the-edit)):

```json
"locks":["@logo","end","$accent","palette","typefaces"]
```

- `@logo`: what the role names. `end`: that layer and everything inside it. `$accent`: that slot of the scene.
- `palette`: an edit may only set colours the scene already has. `typefaces`: an edit may only set font families it already uses.

A lock that names no role, layer or slot is an error at it, `locks[1]`. An edit that touches a lock is refused before it is made, naming the lock: `` `end/cta` is inside `end`, which is locked by `end` ``. `nakkash roles` marks the roles a lock holds.

## Images

An image layer draws a picture file: `image` names a PNG or JPEG file, or vector footage (a PDF file, an `.ai` file that is PDF inside, or an EPS file, whose PostScript program nakkash runs itself, [D71](decisions.md#d71-eps-footage-is-drawn-by-our-own-postscript-interpreter)), relative to the scene file (an absolute path also works), and `box` says where it goes in the layer's own coordinates, `[width, height]` from the origin or `[x, y, width, height]`, as for box text. The picture is stretched to fill the box, so its size in the file does not have to match; an import writes the size the project gave it.

```json
{"id":"photo","image":"photos/beach.jpg","box":[1920,1080],"at":"center center","opacity":0.8}
{"id":"logo","image":"brand/logo.ai","box":[444,662],"crop":[738,209,444,662],"continuous":true}
```

- **Box.** `box` is required: the scene is read, checked and laid out without opening its files, so the layer's size is in the scene. `anchor` rests at the box's centre, and `check` measures the box.
- **`crop`:** the part of the picture stretched over the box, `[x, y, width, height]` from the picture's top-left corner (or `[width, height]` from it), in the picture's own units: pixels for a raster picture, the page's units (points) for vector footage. Nothing outside it shows. Without it the whole picture fills the box: every pixel, or the page's crop box.
- **Vector footage** ([D70](decisions.md#d70-vector-footage-is-drawn-from-its-outlines)) is drawn by nakkash from its outlines, paints, gradients, images and text, never converted to pixels outside the engine. By default its part is drawn once into a picture the box's size (one pixel per unit of the box), which is then transformed like any picture: scaled up, it softens, as a raster picture would. What it holds that cannot be drawn is reported at import.
- **`continuous`:** `true` draws vector footage through the layer's transform at the output's resolution every frame, so it stays sharp however far it is scaled. It changes nothing for a raster picture.
- **Files.** A render reads and decodes each file once, the first time a frame shows it, and draws every later frame from that. A file that is missing, or is not a PNG, JPEG, PDF or EPS file, is an error that names the file; a file no frame shows is never read. The format is recognised by the file's first bytes, not its name.
- **Colour.** PNG keeps its transparency; JPEG is opaque. Pixels are read as sRGB, and sampled bilinearly. Vector footage's colours are converted to sRGB as it is read: device grey and RGB as they are, CMYK through a built-in press profile, ICC-based colours through the file's own profile.
- Like any layer, an image has a transform, parent, opacity, span and time, can be a matte, and can be matted. Video files are not image layers; they are [video layers](#videos).

## Videos

A video layer plays a clip: `video` names an `.mov` or `.mp4` file, relative to the scene file, and `box` says where its frames go, as for an image ([D47](decisions.md#d47-video-layers-play-their-clip-in-their-own-time)).

```json
{"id":"sparks","video":"footage/sparks.mov","box":[1920,1080],"offset":"2s","opacity":0.8}
```

- **Time.** The clip plays in the layer's own time: at its own time `t`, the layer draws the frame showing `t` seconds into the clip. So `offset` starts it later and `stretch` slows it (`2`) or speeds it (`0.5`). Before the clip starts and from its end on, the layer draws nothing.
- **Codecs.** QuickTime Animation (`rle `, RGB, or ARGB with its alpha) and Photo JPEG decode everywhere. H.264 decodes on macOS through the system's decoder ([D60](decisions.md#d60-h264-footage-decodes-through-the-systems-decoder)), in the matrix and range its tags give, or BT.709 limited range for an untagged HD clip; on other platforms it is an error that says so. A clip in any other codec is an error that names the file and the codec. The container is read by nakkash itself, with no system library.
- **Files.** A render opens each clip once and decodes frames as they are shown, keeping the last ones, so a render going forward decodes each frame once. A file no frame shows is never read.
- Like any layer, a video has a transform, parent, opacity, span and time, can be a matte, and can be matted. Its sound is not played yet: an `audio` layer of the same file plays it once AAC can be read.

## Audio

An audio layer plays a sound file: `audio` names a WAV, MP3, FLAC or Ogg Vorbis file, relative to the scene file. It draws nothing, so it needs no box ([D56](decisions.md#d56-audio-layers-mixed-at-48-khz-and-read-by-effects)).

```json
{"id":"music","audio":"music/theme.mp3","offset":"0.5s","span":"0.5s-12s","anim":{"volume":"0 -> 1 @0-1s"}}
```

- **Time.** It plays in its own time, as a video does: at its own time `t` it plays the sound `t` seconds into the file. `offset` starts it later, `span` trims it, and `stretch` plays it slower (`2`) or faster (`0.5`), pitch included, as tape would. Before 0 and from the file's end on it is silent.
- **Volume.** `volume` is a gain, 1 as recorded, 0 silent, up to 4 (+12 dB). It is a property like `opacity`, so a fade is keys. A `comp` layer's `volume` scales everything its composition plays, and an audio layer inside a composition plays while the `comp` layer is in its span, in the composition's time. Only `audio` and `comp` layers take a `volume`.
- **Muting.** A `hidden` audio or `comp` layer is silent.
- **What it is not.** An audio layer has no effects and no matte, and is no layer's matte: it draws nothing.
- **Drawn.** `audio-spectrum` and `audio-waveform` effects on other layers draw its sound, or a composition's through its `comp` layer (see effects).
- **Followed.** A track written `@level <id>` follows its level (see property tracks).
- **The mix.** Every audible layer is mixed into one stereo track at 48 kHz. Files are read by their first bytes, not their names, folded down to stereo, and resampled. Volume moves smoothly between keys, and layers are summed without a limiter. `nakkash render scene.json --out mix.wav` writes the mix, and says when it goes above full scale; a video of the scene holds the mix as its sound (AAC in MP4, PCM in MOV).

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

**Following a sound.** In place of the time range, `@level <id>` makes the track follow the level of a sound: the `audio` layer, or the `comp` layer for its composition's sound, that `<id>` names in the same list. It goes from the first value at silence to the second at full level:

```json
"anim":{"scale":"rest -> rest*1.25 @level music 30-150Hz smooth 0.06s out-quad"}
```

- **The level** at a frame is the sound over a frame's window centred on it, as the mix plays it there, volume and fades included. Without a band it is the RMS level, scaled so a full-scale sine reaches 1. With a band (`<low>-<high>Hz`) it is the loudest part of the spectrum within the band, as a spectrum's bands are measured. Above 1 it counts as 1.
- **`smooth <time>`** lets the level rise and fall towards each frame's with that time constant (a one-pole filter run from the list's first frame), so it does not flicker. It is 0 by default.
- **The curve** shapes the level: `out-quad` lifts quiet passages, `in-quad` keeps only the peaks.
- A track that listens takes two values, which may be written against the rest, on a number, a point or a colour. `volume` cannot listen, since its own sound would change what it hears.
- The engine measures each level once, one a frame of the list's clock, and keys the track every frame, linear between, so any frame evaluates alone.

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

A preset is data: a list of properties and where each starts. The behaviour then animates each one from its start to its resting value. These are built in (`crates/nakkash-format/src/presets.json`):

| Preset | Starts from |
|---|---|
| `fade` | opacity 0 |
| `fade-up` | opacity 0, moved down by the distance, so it rises into place |
| `fade-down` | opacity 0, moved up by the distance |
| `fade-left` | opacity 0, moved right by the distance |
| `fade-right` | opacity 0, moved left by the distance |
| `pop` | opacity 0, scale 0.6 |
| `spin-in` | opacity 0, rotated -90 degrees |
| `rise` | moved down by the distance, already opaque |
| `slam` | scale 1.6, already opaque |
| `push-up` | moved down by the canvas's height; as `out`, it leaves at the top |
| `push-down` | moved up by the canvas's height |
| `push-left` | moved right by the canvas's width |
| `push-right` | moved left by the canvas's width |
| `swing-in` | turned 35° about x and -20° about y, scale 0.9; a 3D layer only |

A start is a number, `"+d"` (the resting value plus the behaviour's distance) or `"-d"` (minus it), or the canvas's size: `"+h"` and `"-h"` move by the height of the canvas the layer sits on (its composition's), `"+w"` and `"-w"` by its width. A push between two beats is one `out` and one `in` at the same moment with the same curve, on the pivot that carries each beat: `{"id":"offer","pivot":true,"span":"0s-2s","out":"push-up 12f in-out-cubic"}` and `{"id":"end","pivot":true,"span":"1.6s-4s","in":"push-up 12f in-out-cubic"}`. A preset that moves a depth row, such as `swing-in`, needs `"3d":true` on the layer. A scene adds presets, or overrides a built-in one by reusing its name, with a `presets` key:

```json
"presets":{"drop":{"opacity":0,"y":"-d","scale":0.8}},
"layers":[{"id":"title","text":"Hi","in":"drop 80px 12f ease-out"}]
```

### Exits

`out` is an exit, the mirror of `in`: `"out":"fade-down 12f"` plays the preset so the layer reaches its pose on the last frame it shows: the last of its span, or of its composition when it has no span ([D72](decisions.md#d72-scenes-written-from-scratch-get-choreography-as-data-and-a-guide)).

- **The pose:** a start relative to the rest is mirrored and an absolute one kept, so a layer leaves the way its entrance arrives: `fade-up` rises into place and rises out, `pop` grows in and shrinks out.
- **When:** the exit ends on that frame, in the layer's own time like its keys, so a push is wholly off the canvas, and a fade wholly clear, on the frame before the layer goes. `@6f` ends it 6 frames earlier. The curve runs from the rest to the pose, so `ease-in` leaves slowly and speeds away.
- **With an entrance:** `in` and `out` may move the same property when the exit starts once the entrance has ended; otherwise the file is rejected at `out`.
- **Not yet:** an exit of the parts of a text (`fade word`), and an exit on a layer that runs backwards (a negative `stretch`), are rejected.

```json
{"id":"offer","text":"Free for a week","span":"2s-4s","in":"fade-up 10f out-expo","out":"fade-up 8f ease-in"}
```

### One writer per property

If `in` or `out` already animates a property, `anim` may not also animate it: the file is rejected, naming the property and the preset. There are two ways out. For `in`, move the entrance into `anim`: set `in=none` and write the preset as `anim` tracks, one per property it sets; the error message gives that edit for the layer, computed from the preset, the layer's `at` and the scene size (custom curves are shown to two decimals). Or define a scene preset under `presets` that leaves the property out. For example `"in":"fade 10f"` with `"anim":{"opacity":"0 -> 1 @0-12f"}` is an error; `anim` on `rotate` is fine.

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

- **`style`** sets character and paragraph keys for the whole text, and the keys of the text level, which only the whole text has. `font` and `fill` stay layer keys.
- **`runs`** override character keys over `range`, `[start, end]` in characters; a later run wins where runs overlap.
- **`paragraphs`** holds one object of paragraph keys per paragraph, in order.
- **`box`** makes box text, which wraps inside the box: `[w, h]` from the layer's origin, or `[x, y, w, h]` from the box's top-left corner in the layer's own coordinates, as imported box text keeps it. Without it the text is point text: each paragraph is one line, aligned about the origin, and the origin is where the first line's baseline starts.

The layer's box is the box its lines cover, which for point text starts above and maybe left of the origin; `anchor` rests at its centre, so `at` centres the text. The keys are listed once, in `crates/nakkash-format/src/text.rs`; this table mirrors that list, and a test checks it.

| Style key | Level | Accepts |
|---|---|---|
| `font` | character | `<family or PostScript name> <size> [<weight>]`, e.g. `Lato-Black 55` |
| `italic` | character | `true` or `false` |
| `fill` | character | a colour, e.g. `#FFFFFF` or `[1, 1, 1, 1]` |
| `stroke` | character | a colour, e.g. `#8F8F8F` |
| `stroke-width` | character | a number of px, at least 0 |
| `paint` | character | `fill-over` or `stroke-over` |
| `tracking` | character | a number, in thousandths of an em |
| `kerning` | character | `font` or `none` |
| `horizontal-scale` | character | a number above 0, e.g. `0.93` |
| `vertical-scale` | character | a number above 0, e.g. `1.04` |
| `leading` | character | `auto` or a number of px |
| `baseline` | character | a number of px |
| `caps` | character | `normal`, `small`, `all` or `all-small` |
| `faux-bold` | character | `true` or `false` |
| `faux-italic` | character | `true` or `false` |
| `lang` | character | a language tag such as `tr` or `pt-BR` |
| `align` | paragraph | `left`, `right`, `center`, `justify-left`, `justify-right`, `justify-center` or `justify-all` |
| `indent-first` | paragraph | a number of px |
| `indent-start` | paragraph | a number of px |
| `indent-end` | paragraph | a number of px |
| `space-before` | paragraph | a number of px |
| `space-after` | paragraph | a number of px |
| `auto-leading` | paragraph | a number, at least 0 |
| `anchor-by` | text | `char`, `word`, `line` or `all` |
| `anchor-align` | text | `[x, y]`, fractions, e.g. `[0, -0.3]` |
| `paint-across` | text | `each`, `fills-over` or `strokes-over` |

`horizontal-scale` sets the characters wider (above 1) or narrower than the font draws them: each advance, the tracking after it and the glyph scale across by it, so a line of `horizontal-scale` 0.93 is 93 % as long. `vertical-scale` draws the glyphs taller or shorter, up from their baseline; a line's ascent grows with its tallest glyphs, while the spacing from one baseline to the next stays.

`faux-bold` thickens the glyphs and adds 30 thousandths of an em after each character, the space the project files' own layouts give it.

`kerning` `font` (the default) spaces each pair of characters by the font's kerning, `none` advances each character by its own width.

`paint-across` stacks fills and strokes across characters. With `each`, the default, every character draws its two paints in its own `paint` order over the characters before it, so a wide stroke can cover a neighbour's fill. `fills-over` draws every stroke and then every fill, so no stroke covers a fill; `strokes-over` draws every fill and then every stroke.

`lang` says what language the words are in. Capitals follow it: with `caps` `all`, Turkish and Azerbaijani (`tr`, `az`) keep the dot on i, so `ücretsiz` is set `ÜCRETSİZ`, where the default makes `ÜCRETSIZ`. The shaper also uses the font's local forms for the language, such as the Turkish forms that keep `fi` apart. Without it, text is cased and shaped as no particular language.

### Text animators

`animate` on a text layer lists animators. An animator's key holds its selectors, and its other keys are what it applies: each part of the text takes those values scaled by how much the selectors choose it. An animator applies only the properties it sets or animates.

```json
"animate":[{"animator":[{"range":"line","anim":{"offset":[{"t":"0.48s","v":0},{"t":"2.76s","v":1,"curve":"ease-out"}]}}],"position":[0,75],"opacity":0}]
```

Above, every line starts 75 px low and invisible. As `offset` moves the range off the lines, from the first line to the last, each line rises and fades in.

- **Selectors.** `range` counts characters (`char`), characters other than spaces (`char-no-space`), `word`s or `line`s; its key holds which. `start`, `end` and `offset` are fractions of the parts, from 0 (before the first) to 1 (after the last). `amount` scales the choice, and a negative amount reverses the animator. With the `square` shape, a part is chosen by how much of it the range covers, and `smooth` 0 makes that all or nothing, decided by the part's middle. The other shapes are read at each part's middle: `ramp-up` rises across the range and stays chosen after it, `ramp-down` is the reverse, and `triangle`, `round` and `smooth` rise and fall inside it. `ease-low` and `ease-high`, from -1 to 1, bend the choice a shape gives, before `amount`: it follows a cubic from 0 to 1 whose first handle lies `ease-low` along the bottom (or, negative, up the start) and whose second lies `ease-high` back along the top (or, negative, down the end). Above 0 the choice leaves 0 or nears 1 more slowly, below 0 more quickly ([D63](decisions.md#d63-a-range-selector-eases-its-choice-by-a-cubic)). `mode` combines a selector with the ones before it. With `units` `index`, `start`, `end` and `offset` count parts instead: 0 is before the first, 1 after the first, `n` after the last of `n`, and fractions of a part cover part of it, so an `offset` of 2 moves the range two parts on.
- **Each part its own amount.** A `parts` selector holds one `part` item per part it counts by `by`, in reading order; each item's key is how strongly its part is chosen, from -1 to 1, and may be animated. Two numbers choose it across and down apart: `position`, `anchor` and `scale` take each axis its own amount, and the animator's other values the first. A part past the last item is not chosen. It combines with the selectors before it by `mode`, as `range` does. An import writes one where the original computed each part's choice by an expression ([D64](decisions.md#d64-a-selector-may-choose-each-part-by-its-own-amount)).

  ```json
  {"parts":[{"part":1},{"part":0,"anim":{"amount":"0 -> 1 @0-1s"}}],"by":"word"}
  ```

- **What applies.** `position`, `rotate`, `skew` and `skew-axis` add. `scale` and `opacity` move from 1 toward the animator's value. `fill`, `stroke` and `stroke-width` move from the part's own toward the animator's. `tracking` adds space after each chosen part, and the parts after it on its line move along, keeping the line's alignment. `blur` adds up, across and down apart (`blur-x`, `blur-y`), and blurs each part in a buffer of its own, by the `blurriness` the `gaussian-blur` effect takes.
- **Each part's anchor** is the middle of its baseline, moved by the animator's `anchor`; parts scale, slant and turn about it, in that order. `skew` slants a part by that many degrees along the direction `skew-axis` degrees clockwise from across: with an axis of 0, a positive skew leans its top forward, as an italic does. The skew is held within -85 and 85 degrees. The text's `anchor-by` style key gives every character the anchor of its word, its line or the whole text instead of its own (a space keeps its own under `word`), and `anchor-align` moves every anchor across by fractions of its part's width and down by fractions of its font's size: `[-0.5, 0]` is the start of the part's baseline, `[0, -0.3]` 30 % of the size above it.
- **Stagger.** With `stagger` above 0, the parts counted by `by` take turns: each part sees the animator, its selectors included, as it was its wait earlier, so every part makes the whole motion with its own curve. A part waits `stagger` frames per turn. `order` sets the turns: `first` in reading order, `last` the reverse, `center` outward from the middle (the two middle parts of an even count go together, half a turn in), `edges` inward from both ends, and `random` a shuffle picked by `seed`. A glyph in no part, such as a space under `word`, sees the animator as it is.

| Animator | Key holds | Properties | Settings |
|---|---|---|---|
| `animator` | its items | `position`, `anchor`, `scale`, `rotate`, `skew`, `skew-axis`, `opacity`, `fill`, `stroke`, `stroke-width`, `tracking`, `blur`, `stagger`, `seed` | `by`: `char`, `char-no-space`, `word`, `line`; `order`: `first`, `last`, `center`, `edges`, `random` |

| Selector | Key holds | Properties | Settings |
|---|---|---|---|
| `range` | `by` | `start`, `end`, `offset`, `amount`, `smooth`, `ease-high`, `ease-low` | `shape`: `square`, `ramp-up`, `ramp-down`, `triangle`, `round`, `smooth`; `mode`: `add`, `subtract`, `intersect`, `min`, `max`, `difference`; `units`: `fraction`, `index` |
| `parts` | its items | | `by`: `char`, `char-no-space`, `word`, `line`; `mode`: `add`, `subtract`, `intersect`, `min`, `max`, `difference` |

| Part | Key holds | Properties | Settings |
|---|---|---|---|
| `part` | `amount` | `amount` | |

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
- **`@level`:** follows a sound's level (see property tracks).

Modifiers are declarative and repeatable, so checks and repeatability keep working. nakkash does not run arbitrary scripts inside scenes (see [D10](decisions.md#d10-modifiers-instead-of-scripts)).

## Fonts and assets

nakkash never goes to the network while rendering. A render depends only on the scene, the files it points to, and the nakkash version.

- **Inter** is built in.
- **Other fonts, images and audio** are referenced by file path.
- **`nakkash fonts fetch <scene>`** finds every face the scene's text uses and its `fonts` lack, in open-licensed sources, and pins each to an exact file in a lock beside the scene (`story.fonts.lock` for `story.json`). It adds the lock to `fonts` and downloads the files into the font cache (`$NAKKASH_CACHE`, or the platform's cache folder). It is a separate, explicit step; `nakkash import --fetch-fonts` runs it for an imported project ([D39](decisions.md#d39-fonts-are-resolved-before-a-render)).
- **A lock in `fonts`** loads each face it pins from the cache and checks the file's hash. A face the cache lacks is an error that names `nakkash fonts fetch`.
- **System fonts** work, but the checker warns about them, because the result then depends on the machine.
- **A missing font or glyph** is an error, never a silent fallback. Only an import substitutes a font it does not have, by writing another face's name into the scene and reporting it ([D40](decisions.md#d40-missing-fonts-are-substituted-at-import)).

## Validation

- **JSON Schema.** A schema is generated from the engine's own types and published, so editors and agents can validate scenes.
- **Errors you can act on.** When a short string does not parse, the error shows what the full, expanded form should look like.

## Versions

`"nakkash": 1` is the format's version. Scenes written before the project was renamed say `"motion": 1`: that is read as the same key, with a deprecation note, and a scene may not have both ([D73](decisions.md#d73-the-project-is-named-nakkash)). It stays 1 while changes only add: a new key, a new value form, a looser limit. A change that alters what a written scene means bumps it, and nakkash reads every older version through migrations ([D36](decisions.md#d36-the-format-only-changes-in-the-open)). The scenes in `crates/nakkash-format/tests/compat/` hold that promise in CI: together they use every name in this document, and what each one draws is a snapshot.

## Why JSON

We measured the same scene in several formats (see the [research note](../research/2026-11-13-baselines.md#formats)). JSON wins on three counts:

- **Density matters more than syntax.** Defaults, presets and short strings cut tokens about 11 times. Switching between JSON, YAML and a custom language changed the count by only 3–15%.
- **Models write JSON reliably.** Published studies find that models produce valid JSON more reliably than YAML.
- **The tooling already exists.** JSON Schema and constrained output come for free.

The short strings inside the JSON look like CSS on purpose, because models already know that style.
