# From `.aep` projects to frames

**Goal:** an imported `.aep` project becomes an ordinary motion scene, and motion draws its frames. We start with the simplest local sample project (four compositions, three text layers, four shape layers, one fill effect) and measure every frame against the project's official reference render.

**Done means:**
- `motion import <file.aep> -o scene.json` writes a scene that `motion check` accepts, plus a mapping report that lists everything the scene could not express. Nothing is dropped silently.
- `motion render scene.json` draws any frame with `vello_cpu`, from fonts given as assets, with no network and no system fonts.
- For the first sample, frames match the reference render within numbers we publish in the research note: masked colour error per frame, coverage overlap, and text geometry against the project's own stored layout.
- Golden-image tests guard the renderer in CI. CI is green, and coverage stays at 85% or more.

## The reference sample

The first sample is a 1080x1920, 25 fps, 15 s story layout: a yellow square rotated 45 degrees slides up, a white square pops in, three lines of text rise one line at a time, and two nested compositions bring in a hashtag and an animated arrow. It uses shape groups with their own transforms, open paths with round-capped strokes, non-uniform scale, a fill effect on text and on nested compositions, a per-line text animator, and expressions on most transforms.

Its reference render is H.264 at 59.94 fps and shows a photo where the project has an empty placeholder composition. So the comparison:
- samples our scene at the reference frame's exact time (fractional frames are native to evaluation);
- measures only where we draw, and separately over the union of everything we draw in the clip, so a missing or late element still counts against us;
- reads text geometry from the project itself (its composed-line cache), independent of video compression.

Sample projects and fonts stay local (see the sample folder's `SOURCES.md`); the repository holds only the tools and the numbers.

## Long-term choices

Each is a decision record (D31 to D35); the summary here says what it changes.

| Decision | What changes |
|---|---|
| [D31](../design/decisions.md#d31-typed-values-and-value-space-keys) Typed values and value-space keys | A property row declares a value type: number, point, colour or path. A track animates a whole value or one component. Keys hold bezier handles in value units (speed and influence), so any ease survives import, including a key that leaves and returns to the same value. Points can move along a curved path with arc-length timing. Named curves and `cubic(...)` stay as authoring forms and compile to the same keys. |
| [D32](../design/decisions.md#d32-one-format-for-authored-and-imported-scenes) One format for authored and imported scenes | Nested compositions, a shape content tree, effects and text animators are scene features. The importer writes ordinary scenes, and every object with properties (a layer, a shape item, an effect, a text animator) has its own `anim`. |
| [D33](../design/decisions.md#d33-text-is-shaped-from-font-assets) Text is shaped from font assets | Fonts are assets, resolved by PostScript or family name from given folders; a missing font is an error. Shaping and line layout use Parley (HarfRust shaping) in a crate of its own, `motion-text`, and compile uses it through the existing `TextMeasure` seam. |
| [D34](../design/decisions.md#d34-a-layer-tree-renderer-on-vello_cpu) A layer-tree renderer on `vello_cpu` | `motion-raster` draws the evaluated scene as a tree: a nested composition is a clipped layer, opacity, blend modes and effects are layer operations. Effects are a registry of name, typed parameters and a draw function. Blending happens in the scene's colour space setting: imported scenes take the project's, authored scenes default to linear light ([D21](../design/decisions.md#d21-linear-light-float-compositing)). |
| [D35](../design/decisions.md#d35-fidelity-is-measured-against-reference-renders) Fidelity is measured against reference renders | `motion diff` compares frame folders and writes per-frame numbers as JSON; the import report and the text geometry check give the rest. |

## Work, one pull request each

| # | Pull request | Crates |
|---|---|---|
| 1 | This plan and D31 to D35. | docs |
| 2 | **Text documents and orientation.** A `winnow` grammar for the text-document body (a PDF-style object syntax), a typed `TextDocument` (text, paragraph and character runs with font, size, colours, tracking, leading, caps, justification; the box frame; the stored line layout), and orientation angles. Both leave the opaque list. | `motion-import` |
| 3 | **Typed values and keys (D31).** Value types in the registry, typed tracks, value-space handles, spatial tangents, components. The transform set: `anchor`, `x`/`y` (the components of `position`), `scale` as a point, `rotate`, `opacity`. Parenting and layer timing (in, out, start, stretch). Checks and `at` keep working on the new model. | `motion-format`, `motion-scene`, `motion-query`, `motion-edit` |
| 4 | **Compositions, shapes and effects (D32).** `comps` and `comp` layers with their own time, `solid` layers, the shape content tree (groups with transforms, paths, rectangles, ellipses, fills, strokes with caps and joins), the effect list with its registry (`fill` first), hidden layers. | `motion-format`, `motion-scene` |
| 5 | **Text (D33).** Text style (font asset, size, fill, stroke, tracking, leading, caps, justification), paragraphs and box text; text animators: selections by character, word or line with range, offset and shape, applying property offsets. `motion-text` shapes and lays out with Parley; Inter ships built in. | `motion-format`, `motion-scene`, `motion-text` |
| 6 | **Renderer (D34).** `motion-raster` on `vello_cpu`: shapes, glyph runs, nested compositions, opacity, blend modes, the `fill` effect. `motion render` writes PNG frames. Golden-image tests. | `motion-raster`, `motion-engine`, `motion-cli` |
| 7 | **Import to scene.** The mapping from the `.aep` model to a scene, as tables from match names to scene properties, with a mapping report. `motion import`. | `motion-import`, `motion-engine`, `motion-cli` |
| 8 | **Fidelity (D35).** `motion diff`, the text geometry check, and a research note with the first sample's numbers; then fix what the numbers show. | `motion-raster`, `motion-cli`, docs |

After this plan: expressions, evaluated with Boa at import and baked into keys so scenes stay script-free ([D10](../design/decisions.md#d10-modifiers-instead-of-scripts)); effects and mattes on a `wgpu` compositor; the 2.5D camera.

## What the first sample needs that we do not have

Read from `motion inspect` on the sample:

| Need | Pull request |
|---|---|
| Text document: font, size, colour, caps, line breaks, justification | 2, 5 |
| Anchor, non-uniform scale, rotation on shape layers and inside shape groups | 3, 4 |
| Position, scale and opacity keyframes with their eases | 3 |
| Nested compositions starting at 1 s and 2 s | 4 |
| Rectangles and an open, round-capped stroke path | 4 |
| The fill effect on text and on nested compositions | 4, 6 |
| A per-line text animator: percent offset over lines, position and opacity offsets | 5 |
| Expressions on most transforms (an ease library and a cycle loop) | after this plan; reported until then |

## Tests

- Every grammar has `proptest` never-panic tests, as before.
- Unit tests build their input by hand (chunks, scenes, graphs); fixture snapshots guard the reader against accidental change.
- The renderer has golden PNGs, compared exactly on the CPU path ([D12](../design/decisions.md#d12-what-repeatable-means)).
- The local sweep over every sample project now also imports each one and sums the mapping reports, so support grows by how often a feature appears.
