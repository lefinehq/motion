Status: Draft · 2026-11-13

# Glossary

The words we use in motion, and what they mean.

**Agent.** An AI system that writes and edits scenes, for example a coding assistant or a content pipeline.

**Behaviour.** A short description of a whole animation, such as `fade-up 12f stagger 6f`. It is the simplest way to animate.

**Check.** A question motion answers about a scene with numbers, without rendering the video. See [Edits and checks](design/edits-and-checks.md).

**Compositing.** Combining layers into one frame, with their effects, masks, mattes and blend modes.

**Core prompt.** The short text (≤ 1,000 tokens) that teaches an agent how to use motion. *Topic pages* add detail on demand.

**Craft lint.** Automatic checks on timing, easing, judder, text hold and layout, computed from the scene. See [Quality](design/quality.md).

**Draw-on.** A path operation that makes a stroke appear to draw itself.

**Edit.** A one-line command that changes a scene, such as `card3 fill=red`.

**Evaluate.** Working out every layer's exact values at a given moment.

**Filter layer.** A layer with no content of its own, only effects. It applies them to everything below it.

**Gate.** A pass/fail check a clip must clear before anyone judges its quality, such as safe areas, flashing or loudness.

**Golden image.** A stored, approved frame. Tests compare new renders against it to catch changes.

**Jitter.** A modifier that adds a repeatable random shake.

**Mask.** A shape that cuts out part of a layer.

**Matte.** Another layer whose transparency or brightness is used as a stencil.

**Modifier.** A rule that adds motion without keyframes: jitter, loop, link or follow.

**Nested comp.** A whole composition used as a single layer inside another.

**Override.** A change stored on one copy in a repeat, such as card 3 being red. The repeat stays a single layer.

**Pivot.** An invisible layer that only has a position, rotation and scale. Other layers use it as a parent, so they move together.

**Pixel check.** A check that renders tiny frames and returns measurements, never images.

**Preset.** A named, ready-made value or animation.

**Rasterise.** Turn shapes and text into pixels.

**Reference clip.** A clip made by the lead motion designer in a professional tool, rebuilt in motion to measure how close we get.

**Repeat.** One layer that produces several numbered copies of an item, filled in from `data`.

**Scene.** The JSON file that describes a video.

**Shutter angle.** How much motion blur a frame gets, named after the film-camera setting.

**2.5D.** Flat layers moved and turned in 3D space and seen through a camera, as opposed to true 3D models.
