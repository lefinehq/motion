Status: Draft · 2026-11-13

# Scene format

A motion scene is a small JSON file that describes a video: its size, length and layers, and what each layer does over time. It is built to be cheap for an agent to write. Defaults cover most values, and short strings say a lot in a few tokens.

## An example

```json
{"motion":1,"size":"1080p","fps":30,"dur":"5s","bg":"#0B0D12",
"assets":{"inter":"font:Inter","logo":"./logo.svg"},
"layers":[
{"id":"title","text":"Launch Week","font":"Inter 96 700","at":"center 260","in":"fade-up 12f"},
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

The syntax shown here is illustrative. The exact grammar is part of the spec work.

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
| `rect`, `ellipse`, `path`, `svg` | Shapes |
| `image` | A picture file |
| `solid` | A flat colour |
| `comp` | A nested composition: a whole scene used as a layer |
| `pivot` | An invisible layer that only carries a position, rotation and scale. Other layers can use it as a parent. |
| filter layer | A layer with only an `fx` list. It applies the effects to everything below it. |
| `camera` | The viewpoint for 2.5D layers |
| `audio` | A sound file placed on the timeline |

Video layers, lights and 3D models come in phase 2. See [Features](features.md).

Every layer can also have:

- **position and transform:** `at`, `anchor`, `scale`, `rotate`, `opacity`, plus `rotateX`, `rotateY` and `z` for depth
- **`parent`:** another layer to follow
- **`blend`:** a blend mode
- **`mask`:** cut-out shapes, with feather and expansion
- **`matte`:** use another layer's alpha or brightness as a stencil
- **`fx`:** a list of effects
- **`mblur`:** motion blur
- in and out points, and a time offset

## Animation, at three levels of detail

All three levels compile to the same thing inside the engine. Use the simplest level that does the job.

**1. Behaviours.** These say what happens in a few words:

```json
"in":"fade-up 60px 12f ease-out @15f stagger 6f"
```

**2. Property tracks.** These animate one value from A to B:

```json
"anim":{"x":"0 -> 200 @0-30f ease-out"}
```

**3. Keyframes.** These give exact values at exact frames, with bezier easing. They are the escape hatch for full control.

We store the highest level that fits. A delay or a stagger therefore stays a single value, and changing it is a single edit.

## Repeats and data

`repeat` makes several copies of an `item`, and `data` fills in what differs between them. The five cards above are one layer, not five.

The copies get numbered ids starting at 1: `cards.1` to `cards.5`, also reachable as `card1` to `card5`.

## Layout

A layer or group can lay out its children in rows, columns or grids, for example `row gap 40`, `col`, `grid 3 gap 24` or `align center`.

Layout is resolved before anything moves. Every animation is therefore relative to where its element sits in the layout.

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
- **`motion fonts add <family>`** downloads an open-licensed font, such as a Google Font or a colour emoji font, into a project folder. It is a separate, explicit step.
- **System fonts** work, but the checker warns about them, because the result then depends on the machine.
- **A missing font or glyph** is an error, never a silent fallback.

## Validation

- **JSON Schema.** A schema is generated from the engine's own types and published, so editors and agents can validate scenes.
- **Errors you can act on.** When a short string does not parse, the error shows what the full, expanded form should look like.

## Why JSON

We measured the same scene in several formats (see the [research note](../research/2026-11-13-baselines.md#formats)). JSON wins on three counts:

- **Density matters more than syntax.** Defaults, presets and short strings cut tokens about 11 times. Switching between JSON, YAML and a custom language changed the count by only 3–15%.
- **Models write JSON reliably.** Published studies find that models produce valid JSON more reliably than YAML.
- **The tooling already exists.** JSON Schema and constrained output come for free.

The short strings inside the JSON look like CSS on purpose, because models already know that style.
