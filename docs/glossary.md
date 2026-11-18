Status: Draft · 2026-11-15

# Glossary

The words we use in nakkash, and what they mean.

**Address.** The one way to name a layer anywhere in a scene: the ids down to it joined by `/`, then any keys inside it, such as `card/title.fill`. `scene` names the scene itself. See [D43](design/decisions.md#d43-one-layer-address-and-edits-per-instance).

**Agent.** An AI system that writes and edits scenes, for example a coding assistant or a content pipeline.

**Batch.** A file that lists the variants of one master, each an id and its edits; `nakkash batch` makes, checks and renders them in turn. See [Edits and checks](design/edits-and-checks.md#batches-of-variants).

**Generated variant.** A variant `nakkash vary` makes by walking a master along an axis (palette, content) or improving it by the check's repairs; it passes when it adds no error to its master's report. See [Edits and checks](design/edits-and-checks.md#generated-variants).

**Behaviour.** A short description of a whole animation, such as `fade-up 12f stagger 6f`. It is the simplest way to animate.

**Check.** A question nakkash answers about a scene with numbers, without rendering the video. See [Edits and checks](design/edits-and-checks.md).

**Compositing.** Combining layers into one frame, with their effects, masks, mattes and blend modes.

**Core prompt.** The short text (≤ 1,000 tokens) that teaches an agent how to use nakkash. *Topic pages* add detail on demand.

**Craft lint.** Automatic checks on timing, easing, judder, text hold and layout, computed from the scene. See [Quality](design/quality.md).

**Curve.** How progress eases between two keys: a name such as `ease-out`, `hold`, or `cubic(x1,y1,x2,y2)`. See [Scene format](design/scene-format.md#curves).

**Draw-on.** A path operation that makes a stroke appear to draw itself.

**Edit.** A one-line command that changes a scene, such as `card fill=red`, or `card/title text="Sale"` for one instance of a composition.

**Evaluate.** Working out every layer's exact values at a given moment.

**Filter layer.** A layer with no content of its own, only effects. It applies them to everything below it.

**Gate.** A pass/fail check a clip must clear before anyone judges its quality, such as safe areas, flashing or loudness.

**Golden image.** A stored, approved frame. Tests compare new renders against it to catch changes.

**Jitter.** A modifier that adds a repeatable random shake.

**Key.** One value of a property at one moment. Its curve says how the value eases to the next key.

**Lock.** What a variant may never change: a role, a layer, a slot, the palette or the typefaces. An edit that touches one is refused. See [Scene format](design/scene-format.md#locks).

**Mask.** A shape that cuts out part of a layer.

**Matte.** Another layer whose transparency or brightness is used as a stencil.

**Modifier.** A rule that adds motion without keyframes: jitter, loop, link or follow.

**Instance.** One `comp` layer's use of a composition. Several layers may show the same composition; each is an instance, and overrides change one alone.

**Nested comp.** A whole composition used as a single layer inside another.

**Override.** A change stored on one instance of a composition, under its `comp` layer's `overrides`, such as `card-2`'s title reading "Sale". The composition and its other instances stay as they are. A repeat will store a change to one copy the same way.

**Pivot.** An invisible layer that only has a position, rotation and scale. Other layers use it as a parent, so they move together.

**Pixel check.** A check that renders tiny frames and returns measurements, never images.

**Preset.** A named entrance, stored as data: the properties it animates and where each starts. A scene can add its own.

**Property.** One animatable value of a layer, such as `opacity` or `rotate`. Every property is listed once in a registry, with a default and a range.

**Rasterise.** Turn shapes and text into pixels.

**Reference clip.** A clip made by the lead motion designer in a professional tool, rebuilt in nakkash to measure how close we get.

**Reference.** A value written `"$id"`, which takes the value of slot `id`: of its own composition, else of the scene. `$$` writes a literal dollar. See [Scene format](design/scene-format.md#slots).

**Repeat.** One layer that produces several numbered copies of an item, filled in from `data`.

**Role.** What a layer or slot does in a master, such as `headline` or `logo`. An edit may target it as `@headline`. See [Scene format](design/scene-format.md#roles).

**Scene.** The JSON file that describes a video.

**Shutter angle.** How much motion blur a frame gets, named after the film-camera setting.

**Slot.** A named, typed value a scene or a composition declares for change, such as `accent` or `title`. Values that follow it are written `"$id"`; an instance can give it its own value in `overrides`. See [D44](design/decisions.md#d44-slots-are-named-values-that-layers-follow).

**Track.** The keys of one property over time. Presets, `anim` strings and keyframe lists all become tracks.

**2.5D.** Flat layers moved and turned in 3D space and seen through a camera, as opposed to true 3D models.
