Status: Draft · 2026-11-13

# Decisions

This is the record of every design decision: what we chose, and why. Other documents link here instead of repeating the reasoning. To change a decision, add a new entry that supersedes the old one. Do not edit the old entry.

## Set by the maintainers

These came from the project's direction, not from analysis.

- **motion makes video.** It is not an interactive animation runtime.
- **The pilot is an AI.** The aim is a professional-grade compositing engine for motion design, driven by an agent.
- **The tools to beat are HyperFrames and Remotion,** on tokens and on render speed.
- **No editor in v1.** Anything an agent can do must be easy to offer in an editor later.
- **3D is in scope.**
- **The repository is public** under the `lefinehq` GitHub organisation and must read as a serious, standalone project.

## Approach

We considered three ways to build motion.

**A. Our own format, checks and compositor, with an existing open-source rasteriser.** *Chosen.*
motion's value lies in the parts the agent touches: the format, the edits and the checks. We build those ourselves and let a proven library turn shapes into pixels.

**B. Build on the Rive runtime.** *Rejected.*
- It is designed for files made in Rive's editor, and for interactive use.
- It has no general blur.

**C. Build everything, including the rasteriser.** *Rejected.*
- A high-quality rasteriser takes years.
- It is not where motion's value lies.

## Design decisions

### D1. 3D in two steps

- **Decision:** 2.5D in v1; true 3D in phase 2. The scene structure supports 3D from day one.
- **Why:** 2.5D covers most motion-graphics 3D at a fraction of the cost. A 3D-ready structure means phase 2 needs no rewrite.

### D2. Vello as the rasteriser

- **Decision:** `vello_cpu` first, behind a small interface.
- **Why:**
  - It was tested by hand on this project: every effect we need in v1 works, at about 6.5 ms per 1080p frame.
  - It is pure Rust, so it ships as a single binary.
- **Rejected:**
  - **Skia:** it has the broadest effects, but a heavy C++ dependency of 20–40 MB, and its GPU backend is mid-migration.
  - **ThorVG:** its text is weak. It stays a candidate for Lottie import.

### D3. CPU-first compositor

- **Decision:** effects, masks and mattes run on the CPU in v1. GPU acceleration is phase 2, unless it is needed earlier to hit the speed goal; that is decided at M2.
- **Why:**
  - one implementation
  - bit-identical frames
  - no GPU start-up delay
  - the same behaviour on servers without a GPU

### D4. Rust core, shipped as one binary

- **Decision:** the engine is written in Rust and distributed through npm and crates.io as prebuilt binaries.
- **Why:** installing motion should not need a browser or a compiler.

### D5. JSON with short value strings

- **Decision:** scenes are JSON with CSS-like short strings. There is no custom language.
- **Why:**
  - Measured: a custom language saved only 3% of tokens.
  - JSON is the format models write most reliably.
  - Its tooling already exists.

### D6. One edit language

- **Decision:** all changes are one-line, id-based edits. JSON Patch is the fallback.
- **Why:**
  - Measured: 9 tokens for an edit, against 29 for JSON Patch and hundreds for a rewrite.
  - A future editor reuses the same commands.

### D7. motion does not call models

- **Decision:** motion ships a command-line tool, a library, an MCP server and `motion prompt`. Any agent drives it.
- **Why:**
  - It keeps the engine neutral.
  - It works with every model.
  - It has no API costs of its own.

### D8. Basic audio in v1

- **Decision:** audio files can be placed, trimmed, faded and set in volume, then mixed into the output.
- **Why:** a video tool without sound is not finished, and basic mixing is cheap.

### D9. Licence

- **Decision:** MIT OR Apache-2.0.
- **Why:** it is the Rust ecosystem's convention, and it matches Vello.

### D10. Modifiers instead of scripts

- **Decision:** scenes use declarative modifiers (jitter, loop, link, follow) instead of arbitrary code.
- **Why:** code would break the checks and repeatability.

### D11. A two-level rules prompt

- **Decision:** a core prompt of ≤ 1,000 tokens, plus topic pages of ≤ 400 tokens each, loaded only when a scene needs them.
- **Why:** the full feature set cannot fit in 1,000 tokens, and most scenes need only the core.

### D12. What "repeatable" means

- **Decision:** on the CPU path, frames are bit-identical for the same scene, version, OS and CPU type. Encoded files are not covered.
- **Why:** hardware encoders and GPUs are not deterministic.

### D13. Codec licence policy

- **Decision:**
  - no GPL code and no patent-encumbered software encoders in the binary
  - H.264 and HEVC from the operating system or GPU, or from Cisco's OpenH264 downloaded on first use
  - AV1, VP9 and ProRes always available
- **Why:** this keeps the permissive licence clean and avoids patent exposure.

### D14. No network while rendering

- **Decision:** Inter is built in. Other fonts are downloaded only through an explicit command.
- **Why:**
  - renders are repeatable and work offline
  - fonts are licensed cleanly

### D15. WebAssembly-ready core

- **Decision:** the parts that do not draw pixels stay WebAssembly-compatible.
- **Why:** a browser editor or preview can reuse them later at little cost.

### D16. Quality gates

- **Decision:**
  - text is checked against HarfBuzz glyph positions
  - overall look is signed off by the lead motion designer once per milestone, then protected by golden images
- **Why:** text correctness can be measured, but taste needs a person.

### D17. Staged releases

- **Decision:** 0.1 after M1, 0.2 after M2, and 1.0 after the agent proof.
- **Why:**
  - It gives early users something useful.
  - It gives the project a visible track record.

### D18. Naming rule

- **Decision:** no third-party product names or trademarks in the API, feature names, presets, docs or marketing. Our vocabulary is our own: nested comp, pivot, filter layer, matte, draw-on, jitter. Other tools are named only in factual, reproducible benchmark comparisons, and Lottie only as an import format. Desktop project files are referred to by their file extension (`.aep`).
- **Why:** it avoids trademark trouble, and motion should stand on its own terms.

### D19. Content rule

- **Decision:** the public repository contains no internal product names, no customer data and no links to private repositories.
- **Why:** the repository is public.

### D20. Quality is measured in five layers

- **Decision:** gates, fidelity tests, craft lint, requirement checks, and blind human votes. Everything except taste is automated; taste is judged by people only. The lead motion designer owns presets, reference clips and thresholds. Independent raters judge blind tests. Full plan in [Quality](quality.md).
- **Why:**
  - No existing metric is validated on motion graphics.
  - Pairwise human votes are the most reliable signal per unit of effort.
  - A board run by motion is only credible if competitors can rerun it.

### D21. Linear-light float compositing

- **Decision:** layers are blended in linear light at 16-bit float precision, and output is dithered. Motion blur is true motion blur, and positions are sub-pixel.
- **Why:** this is where browser-based rendering loses quality: muddy glows and blurs from 8-bit gamma-space blending, banding in gradients, and jitter in slow moves. Building it in from the start costs little and costs agents no tokens.

### D22. No AI judge

- **Decision:** no vision-language model scores clips, rankings or quality claims.
- **Why:** AI judges agree with people only about 75–83% of the time on motion graphics. They miss fine detail, and they carry biases of position, length and model family. Quality claims rest on measurable checks and human votes.

### D23. v1 is variant-first

- **Decision:** v1 turns designed projects into variants. Creating videos from scratch gets its own roadmap afterwards.
- **Why:**
  - The master is made by a professional, so quality starts high.
  - Variants are where most of the repetitive work is.
  - Real projects give exact ground truth for testing.

### D24. Import through an exporter inside the desktop tool (replaced by D30)

- **Decision:** an exporter runs inside the desktop tool and uses its documented scripting API. A direct `.aep` reader comes later, after a legal review.
- **Why:**
  - The tool reports every value, so the export is complete.
  - Using the public API carries little legal risk.
  - The tool's own render becomes ground truth for fidelity tests.

### D25. Variants work on a variant surface

- **Decision:** agents see and edit a compact summary of the master (roles, beats, editable values, style profile and locks), not the raw keyframes.
- **Why:**
  - It keeps tokens low.
  - Locked elements can't be touched by accident.

### D26. Reinterpretation stays inside the master's style

- **Decision:** when the AI reinterprets a master, it may only use easings, durations, staggers, transitions, colours and type from the master's style profile.
- **Why:** variants then move the way the designer moves. A style check makes this measurable.

### D27. One generic property model

- **Decision:** every animatable value is a registered property with a keyframe track. Curves, presets and shorthands are data that expand into tracks.
- **Why:**
  - Masters can use any curve on any property.
  - Adding a preset is a data change, not a code change. Adding a property is a registry row for parsing, `anim`, presets, edits, `at` output and clamping. A property with a geometric or visual effect also needs code in `NodeState::bounds()` and the renderer.
  - Evaluation, checks and the future renderer see one model.

### D28. Standard parsing tools and one diagnostic type

- **Decision:** parsing uses standard tools, and every failure is one error type.
  - The scene's shape is read with serde. Text shorthands and the edit line are `winnow` grammars built from shared pieces.
  - Every crate returns `Diagnostic` (a kind, a path, a message and an accepted form). No other public error type exists, and no code formats an error by hand.
  - Tools reach the pipeline only through one entry point, the engine's `Session`. Each step runs in a `tracing` span, so logs and timings come from one place, and libraries never print.
- **Why:**
  - Hand-written tokenizers and `Value` walking grew, and each one phrased its errors differently.
  - An agent fixes a scene from the error text alone, so every error needs a path and the accepted form.
  - One entry point means the command line, an agent server and a Node binding behave the same and are timed the same.
  - Logs are off by default, so command output stays clean.

### D29. Few crates, short comments

- **Decision:** a piece of the pipeline gets its own crate only when it has its own dependencies or its own users.
  - The error type and the shared grammar pieces live in `motion-format`, which every other crate already depends on.
  - Evaluation lives in `motion-scene`, next to the graph it reads.
  - The logging subscriber lives in `motion-engine`, next to the step spans it times.
  - Comments say what an item is and why it exists, in a line or two; they do not narrate the code.
- **Why:**
  - Ten crates for a reader, a checker and an editor was more structure than code: every change crossed several manifests.
  - Long comments repeated the code and went stale with it.
  - The renderer, encoder and importer will be crates of their own, because they bring heavy dependencies.

### D30. Read `.aep` projects directly

- **Decision:** motion reads `.aep` project files itself, with no desktop tool installed and no exporter. This replaces D24.
  - The reader works from the files alone and from public format notes, never from the desktop tool's code.
  - Whatever the reader cannot decode yet stays in the model as an opaque value with its reason, and the import report lists it. Nothing is dropped silently.
  - Binary records are read with `binrw`, the binary counterpart of the text grammars in D28.
- **Why:**
  - Designers and agents can import a project on any machine, including servers where the desktop tool cannot run.
  - Reading a file format for interoperability is well established, and the format is already documented in the open.
  - The report makes the gap measurable, so support grows by how often a feature appears in real projects.

### D31. Typed values and value-space keys

- **Decision:** each property row declares a value type: number, point, colour or path. A track animates a whole value, or one component of a point (`x` and `y` are the components of `position`). Keys hold bezier handles in value units, written as speed and influence on each side of a key; a point key can also hold spatial tangents, and then the value moves along a curved path at a speed measured along that path. Named curves and `cubic(...)` stay as authoring forms and compile to the same keys. This extends D27.
- **Why:**
  - Imported masters use eases that a normalised curve cannot hold: a key that leaves a value and returns to it has no normalised form.
  - A motion path is one curve in space, not two independent curves per axis; splitting it changes the motion.
  - Colours and paths animate too, and one track type over typed values keeps evaluation, checks and edits generic.

### D32. One format for authored and imported scenes

- **Decision:** nested compositions, the shape content tree, effects and text animators are features of the scene format. The importer writes ordinary scenes. Every object with properties (a layer, a shape item, an effect, a text animator) carries its own `anim`. What a scene cannot express yet is listed in the import's mapping report.
- **Why:**
  - Variants, checks and edits then work the same on an imported master and on a scene an agent wrote.
  - A separate import-only model would grow a second engine.
  - The mapping report keeps the gap measurable, as the reader's report does (D30).

### D33. Text is shaped from font assets

- **Decision:** fonts are assets, found by PostScript or family name in folders the caller gives; a missing font or glyph is an error. Shaping and line layout use Parley, with HarfRust shaping, in a crate of its own, `motion-text`. Compile takes a `motion_text::Typesetter` and sets every text layer once, since a layout does not animate. Inter is embedded, so compiling never reads files; the engine reads the fonts a scene's `fonts` lists. (Until 2026-11-14 compile went through a `TextMeasure` seam with an estimate; text animators need the real glyphs, clusters and lines, so the seam went.)
- **Why:**
  - Real shaping is required for every script and for exact line breaks; an estimate cannot be checked against a reference.
  - System fonts make a render depend on the machine (D12, D14).
  - Projects store their own text layout, so our layout can be measured against it.

### D34. A layer-tree renderer on `vello_cpu`

- **Decision:** `motion-raster` draws the evaluated scene as a tree. A nested composition is a clipped layer, and opacity, blend modes and effects are operations on layers. Effects are a registry: a name, typed parameters and a draw function. Blending happens in the colour space the scene sets: an imported scene takes the project's setting, and an authored scene defaults to linear light (D21).
- **Why:**
  - `vello_cpu` already provides isolated layers, clipping, masks and blend modes, so the tree maps onto it directly (D2).
  - Matching a master means blending the way the master was made; projects that do not linearise blend in gamma space.
  - A registry keeps effects as data, so adding one is adding a row and a function, never a branch in the renderer.
- **Status (2026-11-14):** `vello_cpu` blends in gamma space only, so `motion-raster` draws gamma scenes and rejects linear light with a diagnostic. Linear light needs paints and layer buffers converted before blending; that is open work. The format keeps effect kinds (names, properties, settings) and the renderer keeps their draw functions, joined by name, with a test that every format effect has a renderer row. Since 2026-11-14 the engine draws scenes: its `render` module maps evaluated frames to display lists, joining effects, paints and their settings to the rasteriser's tables by name. Scenes have no colour-space key yet, so every frame blends in gamma space.

### D35. Fidelity is measured against reference renders

- **Decision:** `motion diff` compares our frames with a reference render and writes per-frame numbers as JSON: colour error where we draw, colour error over the union of everything we draw in the clip, and coverage overlap. Text geometry is checked separately against the layout stored in the project. The numbers go into a research note for each sample.
- **Why:**
  - Reference renders are compressed and may show content we do not have (a photo in a placeholder), so a plain image difference is misleading.
  - Measuring over the union of what we draw catches elements that are missing or late, not only ones drawn wrongly.
  - Stored text layout is exact, so text can be held to a stricter limit than video allows.
- **Since D42:** a third window, over the reference's content, catches what we fail to draw, and is the headline number.

### D36. The format only changes in the open

- **Decision:** `"motion": 1` holds while changes are additive: a new key, a new value form, a looser limit. A change that alters what an existing scene means, or rejects one that was valid, bumps the version and adds a migration from the previous version to `motion-format`, so every older version stays readable through the chain. A compatibility corpus (`crates/motion-format/tests/compat/<version>/`) holds scenes whose parsed document and frame hashes are snapshots, and which together use every name the format knows: every scene, composition and layer key, every property at rest and animated, every item kind with its properties and each choice of its settings, every style key and every curve form. Until 1.0 a breaking change is allowed, but only with a changed snapshot in the same pull request.
- **Why:**
  - Scenes outlive the code that wrote them: templates, agent prompts and user projects all hold them, and their number only grows.
  - A snapshot makes a change of meaning visible in review, where a silent one would surface months later in someone's render.
  - A coverage test makes the corpus grow with the format: a new key cannot land without a scene that uses it.
  - Early, a breaking change costs one snapshot update; later it costs a migration. The rule is the same, so nothing changes at 1.0 but how often we pay.

### D37. Engines stay behind our own types

- **Decision:** a crate's public API may use vocabulary crates built to be shared between libraries (`kurbo` for geometry, `peniko` for paint, `linebender_resource_handle` for font bytes), named from those crates. Engines stay private: the shaper (`parley`), the rasteriser (`vello_cpu`), the binary and text parsers (`binrw`, `winnow`), and later the JavaScript engine. An engine's own type that reaches a public API becomes our own type. The one case found, `GlyphRun.font`, was the shared font bytes type re-exported through the shaper; it is now named from its vocabulary crate, so the shaper no longer appears in any public API.
- **Why:**
  - Young engines change their APIs often. Behind our types, an upgrade or a replacement touches one crate.
  - Vocabulary crates exist to be shared, and wrapping them would only add conversions.

### D38. Expressions are baked at import

- **Decision:** an expression is evaluated when the project is imported, by an embedded JavaScript engine, at every frame of its layer's span. Its values are written into the scene as keys, fitted to curves within a tolerance. The import report keeps each expression's source and says what it became. An expression that reads something the import cannot know, such as a value the user sets later, is reported, not guessed. A render never runs JavaScript: the engine sits behind motion-import's `bake` feature, which only the CLI turns on, and a test fails if any other crate's resolved dependencies reach it.
- **Why:**
  - Renders stay deterministic, fast and safe on untrusted scenes ([D10](#d10-modifiers-instead-of-scripts)).
  - An imported scene stays an ordinary scene ([D32](#d32-one-format-for-authored-and-imported-scenes)): its motion is keys that people and agents can read and edit.
  - Most template expressions are easing and wiggle scripts over fixed keys. Baked, they lose nothing.
- **Not now:** live expressions in the format. They would be a separate decision, made only if a real template needs a value to follow an edit.

### D39. Fonts are resolved before a render

- **Decision:** a scene's fonts are resolved by a separate command, `motion fonts fetch` (or `motion import --fetch-fonts`), never by a render.
  - The command writes a lock beside the scene, `<scene>.fonts.lock`, and the scene names it in `fonts`. For every face the lock records the source repository, the exact tag or commit, the path, the version, the licence, and the file's SHA-256 and size.
  - Files are kept in a cache by their hash. A render loads each locked face from the cache and checks the hash. A missing or changed file is an error that names the command to run.
  - Sources are data: each family's own repository, whose tags are its releases, and Google Fonts' folder for it, whose commits are. Only open licences are fetched (OFL, Apache, UFL). Other fonts are reported, never fetched.
  - With a project's stored layout as evidence, each family takes the release whose glyph ids and line widths match it best. Otherwise it takes the newest release that has its faces.
  - Fetching lives behind the `fetch` feature of `motion-fonts`. Only the CLI turns it on, and a test fails if any other crate depends on a network client.
- **Why:**
  - Renders stay repeatable and offline ([D12](#d12-what-repeatable-means)). The same lock always draws with the same bytes, on any machine.
  - The version matters. One sample's template was made with Montserrat 3.100 from the family's repository, a release Google Fonts never shipped. Every later version, and every Google Fonts one, gives its words other glyphs.
  - Hashes make the cache safe to share, and the lock makes a scene portable without carrying font files.
- **Not now:** substitutes for licensed fonts, such as a metric-compatible face standing in for Arial. That needs a face to answer to another name, and is a separate decision.

### D40. Missing fonts are substituted at import

- **Decision:** when an imported project names a font the import does not have (the fonts it was given, any it fetched, and the built-in Inter), the import writes the closest face it has into the scene, and reports every use.
  - The closest face is one of the same family if any is available, else the built-in Inter. Of those, it is the one with the same slant if any has it, then the nearest weight, the lighter on a tie.
  - An available face's weight and slant are read from the face itself. A missing font's are read from its PostScript name's style suffix (`-Bold`, `-SemiBoldItalic`, `-Black`) through one table in `motion-text` (`fallback.rs`). A name without a known suffix is regular and upright.
  - Each use is listed under unsupported as `missing fonts, substituted (`<font> → <face>`)`, with where. The ledger and the corpus ranking count it like any other gap.
  - The scene format does not change ([D36](#d36-the-format-only-changes-in-the-open)): the scene names the substitute by its own name. A scene written by hand that names a font the scene lacks is still an error ([D33](#d33-text-is-shaped-from-font-assets)).
- **Why:**
  - Commercial fonts cannot be fetched ([D39](#d39-fonts-are-resolved-before-a-render)). Without a substitute, one missing font made a whole project unusable: 56 of the 198 local templates failed on it.
  - The import knows which fonts it has; a render does not guess. The substitute is written down, so the scene renders the same everywhere and can be edited to another font.
  - Reporting each substitute keeps the gap measurable, as the rest of the mapping report does ([D32](#d32-one-format-for-authored-and-imported-scenes)).
- **Not now:** metric-compatible substitutes chosen by measured widths, and faces that answer to another font's name (see D39).

### D41. Image layers name their file and carry their box

- **Decision:** a layer draws a still picture with `"image":"<file>"`, a PNG or JPEG file relative to the scene file, and the layer's `box` (the key box text already has) says where it goes: `[width, height]` from the origin, or `[x, y, width, height]`. The picture is stretched to fill the box. `box` is required. The file is decoded when a frame first shows it, once per session, by the engine; the rasteriser receives decoded pixels and never reads files. An import writes the size the project gives the footage, and the path of the file it found beside the project.
- **Why:**
  - The scene stays checkable without its files: compile, `check`, `at` and anchors all need the layer's size, and those steps never read files (the same rule as text, [D33](#d33-text-is-shaped-from-font-assets)). A size taken from the file would make a scene's layout depend on a file that may be missing or replaced.
  - Reusing `box` keeps one way to say "this layer's box" across kinds, and its four-number form already covers a box that does not start at the origin.
  - Stretching to the box keeps a render exact when a file is swapped for a smaller or larger one (a proxy, a variant's photo): the layout does not move.
  - A shorthand such as `"photo.jpg 1920x1080"` was rejected: file names hold spaces, so splitting the string is guesswork.
- **Not now:** video (since [D47](#d47-video-layers-play-their-clip-in-their-own-time)) and image sequences, fit modes other than stretch (`contain`, `cover`), colour profiles other than sRGB, and filtering beyond bilinear for strong reductions.

### D42. Fidelity is headlined over the reference content

- **Decision:** `motion diff` measures a third window besides `drawn` and `union` (D35): `content`, per frame, the pixels where the scene draws or where the reference differs from the scene's background colour by more than the tolerance. Where an ignored layer draws and nothing else of the scene does, the reference is left out, as the layer is. `content_failing` is the headline number of `motion diff`, `motion fidelity` and the ledger, and the ledger's regression check counts it like the others. `drawn` and `union` stay.
- **Why:**
  - `drawn` and `union` only look at pixels the scene draws, so content it fails to draw was never measured. A scene that draws nothing in its settled frames scored a perfect 0: in the local corpus of 198 templates, 31 of 39 passing samples passed only that way.
  - Where the scene draws nothing, its frame is its background, so a reference pixel past the tolerance from it is content the scene misses. The window needs no new mask machinery: one comparison, with the same tolerance as the failing pixels.
  - `drawn` and `union` still tell layout error (drawn in the wrong place or colour) apart from missing content (absent from `drawn`, present in `content`).
  - An ignored layer stands for content the reference has and the scene does not, such as a photo in a placeholder, so measuring the reference there would count a gap the manifest declared.

### D43. One layer address, and edits per instance

- **Decision:** a layer anywhere in a scene has one address, and an edit through a comp layer changes that instance only.
  - **Layers.** A layer is named by the ids from the scene down through the comp layers that show it, joined by `/`: `card/title` is the layer `title` inside the comp that the layer `card` shows. Ids cannot hold `/`, so the address is never ambiguous. A top-level layer is its id, as before.
  - **Keys.** Keys inside a layer follow `.`, as edits write them today. An element of a list is chosen by its id, `fx[glow].radius`, or by its index, `runs[0]`.
  - **The scene.** The scene itself is `scene`. No layer may take that id, and the import gives such a layer `scene-2`.
  - **One type.** The address is one type in `motion-format`, with one grammar and one printer. Diagnostics, edits, `at`, `check`, the import report, the fidelity tools and the export all carry it and print it the same way, so an error's place can be pasted into an edit.
  - **Edits per instance.** An edit through an address such as `card/title text="Sale"` is written on the comp layer `card` as an override: `"overrides": {"title": {"text": "Sale"}}`. The comp, and every other layer that shows it, stay as they are. An edit of the comp itself, which every instance shows, is a key path from the scene: `scene comps.label.layers[title].text="Sale"`.
- **Why:**
  - In our local corpus of 198 templates, 121 keep all their text inside nested comps, and today nothing can address it: edits, `at` and `check` reach top-level layers only (measured, 2026-11-15; see the [audit](../research/2026-11-15-architecture-audit.md)).
  - Layers are named four ways today (`layers[1].rect` in errors, `comp/layer` in the import report, a pair in the text geometry, a bare id in edits). An agent cannot carry a place from one tool to another.
  - In 41 of the 198 templates, a comp that holds text is shown more than once, such as one label comp used for three labels. A variant gives each its own text. An edit that changed the comp would change all three, and one that was refused would leave no way to do it.
  - Overrides are the mechanism the slots of D44 need as well: a slot is a named value that overrides point at.
- **Not now:** overriding a comp layer's own size, length or frame rate, and layers added through an override. An override sets keys of layers the comp already has.

### D44. Slots are named values that layers follow

- **Decision:** a scene and each composition declare `slots`: named values typed by `ValueType`, with an optional label. Any value written `"$id"` follows a slot, resolved in the composition's own slots first, then the scene's; `$$` is a literal dollar. A `comp` layer sets its composition's slots for itself in `overrides` (`"$title":"Sale"`). References resolve before the scene is read, on a copy, so nothing after that sees one and the file keeps them. `motion slots` lists them, and `motion edit` sets one with `$id=value`. The import turns the controls that expressions only read into slots, and those expressions into references, when the value they read never changes; the rest stay baked ([D38](#d38-expressions-are-baked-at-import)), and a layer of controls only that draws nothing is left out. An export writes a slot back into its control. This refines D38's "not now". See the [plan](../plans/2026-11-17-slots.md).
- **Why:**
  - Half of the corpus's expressions (3,730 of 7,404, in 125 of 198 templates) read a control and do nothing else. Baked, the one value a designer exposed is copied into every key that read it, and a variant has to find them all.
  - A slot is a value, not a script. Renders stay deterministic and free of JavaScript, and an agent changes a template by setting a handful of typed values it can list.
  - Overrides ([D43](#d43-one-layer-address-and-edits-per-instance)) already give an instance its own values. Slots are the names those values go by, so one mechanism serves both a control layer and a precomposition's exposed values.
  - Resolving at compile keeps the graph, the evaluator and the renderer unchanged.
- **Not now:** arithmetic on slots, checkbox and menu choices, animated slots and seeded wiggles. Each waits for its own measured need.

### D45. Export patches the project it came from

- **Decision:** a scene imported from an `.aep` goes back by patching that same file. `motion export` imports the original again, compares the edited scene with it, and writes each changed value into the bytes it was read from; every other byte stays as it was. A value is written back only where the import knows its source. Anything else that changed is reported, with why, and not guessed. The export checks itself: it reads the patched file again, imports it, and compares the result with the edited scene.
- **Why:**
  - The format is undocumented. We read it by its structure, and pass over what we do not understand (some 20 kinds of chunk per project). Writing a project from nothing would need all of them right; patching needs only the values we change, so what we do not understand is never at risk.
  - The check needs no other tool: our own reader verifies what our writer wrote, in CI.
  - A generated script for the original tool was the alternative. It needs that tool to run it and to test it, and adds a step for the user. It remains the way to export a scene that was not imported, if that is ever needed.
- **Not now:** scenes that did not come from a project, new layers and items, and values the project leaves at their default (they have no bytes to patch). Text and keys come in later steps of the [plan](../plans/2026-11-15-aep-export.md).

### D46. Video is encoded in a crate of its own, behind one codec trait

- **Decision:** `motion-encode` turns drawn frames into video files and knows nothing about scenes. Frames become BT.709 Y′CbCr 4:2:0 in one function of our own; each codec implements one `VideoEncoder` trait; each container one `Muxer` trait. MP4 is our own writer on `mp4-atom`'s boxes, so packets go to disk as they come; `muxide` was weighed and left out because it holds every packet in memory until the end. AV1 through `rav1e` is the codec every platform has. H.264 and HEVC come from the operating system or GPU, or from Cisco's OpenH264 binary loaded at run time, never compiled in ([D13](#d13-codec-licence-policy)). See the [plan](../plans/2026-11-16-encode.md).
- **Why:**
  - A video file is the product. Today it needs another tool, which agents and servers may not have.
  - AV1 in pure Rust under a permissive licence runs everywhere, CI included, so the whole path is tested before any platform encoder exists.
  - Behind the traits, a codec or container can be added or replaced without touching the others or the renderer, and the render crates never link an encoder.
  - Colour conversion in one place means every codec gets the same planes, so codecs differ only in compression.
- **Not now:** audio, WebM, and the Windows and Linux GPU encoders. Each is a later step of the plan.

### D47. Video layers play their clip in their own time

- **Decision:** a layer draws a video clip with `"video":"<file>"`, an `.mov` or `.mp4` file relative to the scene file, stretched over the layer's required `box`, as an image is ([D41](#d41-image-layers-name-their-file-and-carry-their-box)). At its own time `t` the layer draws the frame showing `t` seconds into the clip, and nothing before the clip starts or from its end on. Clips are read by `motion-video`, with our own reader of the MP4 and QuickTime containers and one `Decoder` trait a codec implements; QuickTime Animation and Photo JPEG are the first. The engine opens each clip once per session and keeps its decoder and last frames, so a render going forward decodes each frame once. H.264 comes later under [D13](#d13-codec-licence-policy): from the operating system, or a decoder loaded at run time, never compiled in; it now decodes through the system's decoder on macOS ([D60](#d60-h264-footage-decodes-through-the-systems-decoder)). See the [plan](../plans/2026-11-16-video.md).
- **Why:**
  - The layer's own time is the time every other layer already has, so `offset`, `stretch` and the span place and retime a clip with no new keys, and an import's start time and stretch carry over unchanged.
  - A required `box` keeps the scene checkable without its files, for the reason D41 gives.
  - The crates that read MP4 and QuickTime together are copyleft or pull in system libraries; the sample table is a few hundred lines of our own.
  - QuickTime Animation and Photo JPEG are the corpus's overlays and mattes, with alpha, and decode in pure Rust under our licences; they draw in 10 of the local samples today (72 layers).
  - Decoding behind one trait, as encoding is ([D46](#d46-video-is-encoded-in-a-crate-of-its-own-behind-one-codec-trait)), lets H.264 land as a module without touching the layer or the engine.
- **Not now:** H.264 and other codecs, image sequences, footage loops and frame-rate overrides from a project's interpretation, fit modes other than stretch, and sound.

### D48. Unsafe code lives only in platform bindings

- **Decision:** every crate takes the workspace's lints, which forbid unsafe code, except `motion-videotoolbox`, which calls macOS's VideoToolbox, CoreMedia and CoreVideo through their generated `objc2` bindings. That crate denies unsafe operations inside unsafe functions, and every unsafe block carries a `SAFETY` comment (Clippy's `undocumented_unsafe_blocks` and `multiple_unsafe_ops_per_block` are errors). Its API is safe and names none of the frameworks' types, and `motion-encode` puts it behind its codec trait ([D46](#d46-video-is-encoded-in-a-crate-of-its-own-behind-one-codec-trait)). A test fails when any other crate leaves the workspace's lints. A binding for another platform (Media Foundation, VA-API) or for the system's decoders ([D47](#d47-video-layers-play-their-clip-in-their-own-time)) gets the same treatment: one crate per platform, small, with each block explained.
- **Why:**
  - The system's encoders are fast and their licences cover H.264 and HEVC ([D13](#d13-codec-licence-policy)), but they can only be reached through C and Objective-C calls, which Rust marks unsafe.
  - Kept to one small crate, the unsafe code can be read in full in review. Everything else keeps the guarantee that it holds none.
  - With one unsafe operation per block and a reason for each, a wrong assumption is visible where it is made.
- **Not now:** bindings for Windows and Linux; the system's decoders.

### D49. Effects that read pixels run on a canvas of their own

- **Decision:** an effect row either moves its layer's content (`Apply::Move`) or changes the pixels it drew (`Apply::Pixels`). A pixel effect may also give a way to paint the same change without reading anything, such as `fill`.
  - **Layers drawn as they are.** A layer whose effects all move or paint is drawn in place: moves fold into the content's transform in order.
  - **Layers drawn on a canvas.** Any other layer is drawn on a canvas the size of the frame. Its effects run in order over the pixels, a move then resampling them. The result is drawn back through the layer's opacity, blend mode, clip and matte.
  - **Effect space.** Points and lengths in effect parameters are in the layer's own coordinates. They reach the pixels through the layer's transform (`Space`), so an effect sits on the layer as it lands on the frame.
  - **Adjusting layers.** An `adjust` layer, or a filter layer (`fx` only), applies its effects to what its composition drew below it. The result is mixed back by its opacity, its matte and its content's alpha; a filter layer, having no content, applies everywhere.
- **Why:**
  - Colour corrections, blurs, glows, shadows and distortions read neighbouring or current pixels. The rasteriser cannot read its own buffer while drawing, so those layers need their own canvas. Mattes already draw that way.
  - Running effects where the layer lands keeps one canvas per frame size and one place where effects meet the frame. Mapping parameters through the transform keeps them in the layer's units, as a project stores them. This is exact for moves, turns and uniform scales, which is how imported layers are placed.
  - In a project, an adjustment layer applies where its own alpha is: a solid's box, or a shape's. Applying it everywhere was measured to make frames worse where the region was smaller, so the region stays the layer's content.
- **Not now:** effects on a canvas smaller than the frame (the layer's bounds grown by what the effect reaches). That is a speed-up, measured when blurs land.

### D50. Roles name what a master's layers do

- **Decision:** a scene's `roles` map role names to a layer address or a slot: `{"headline":"title","cta":"end/cta","accent":"$accent"}`. An edit may target a role, `@headline text="Sale"`, and `motion roles` lists them with what each holds. The import proposes roles, from an `@role` prefix in a layer's name or from a vocabulary of names when one layer alone matches, and reports each proposal for the designer to confirm. See the [plan](../plans/2026-11-18-variant-surface.md).
- **Why:**
  - A brief speaks of the headline and the call to action, not of `card-2/title`. Edits against roles let one brief run on many masters, and keep the agent's prompt the size of the roles, not of the scene.
  - Names find a role in a minority of corpus scenes (a logo in 43 of 196, a headline in 27), so roles are data the designer confirms, not a guess made at every run.
  - Naming a layer by its address, or a value by its slot, reuses the two ways a scene already names things.
- **Not now:** inferring roles from content and position, and text limits per role.

### D51. Locks are refused in the edit

- **Decision:** a scene's `locks` list what a variant may not change: a role (`@logo`), a layer and everything inside it (`end`), a slot (`$accent`), `palette` (only colours the scene already has) and `typefaces` (only font families it already uses). `apply_edit` refuses an edit that touches a lock and names the lock. The import writes `@logo`, `palette` and `typefaces` when it finds a logo. A batch cannot edit `locks` or `roles`.
- **Why:**
  - Refusing at the edit means no variant ever holds a locked change, and the agent learns why at the step that caused it, in the same message an edit already returns.
  - The named locks compare an edit with the scene it edits, so they need no stored lists, and any number of edits stays inside the master's palette and typefaces.
  - Locks belong to the designer, who can still change them with an ordinary edit; a batch is the agent's, so it cannot.
- **Not now:** locks on timing and motion, which wait for beats and the style profile.

### D52. A batch makes every variant from one master

- **Decision:** `motion batch variants.json` reads a master and a list of variants, each an id and edit lines, with optional render settings. Each variant is the master with its edits applied in order through the locks, then checked; one with a refused edit or a check error is reported and not rendered. The others render, one at a time, to `<out>/<id>.mp4`, with their scene beside it. The report is one line per variant, or data with `--json`, and `--check` stops before rendering.
- **Why:**
  - Variants differ from the master by a handful of edits. Keeping them as edits, not as copies of the scene, keeps a batch reviewable and its cost in tokens the size of the brief.
  - Reading the master and loading its fonts once, then rendering in turn, keeps every core on one render, which already uses them all.
- **Not now:** variants in parallel, and variants of variants.

## Open questions

### Q1. Name and packages (deferred)

The npm package `motion` and the Rust crate `motion` are taken, and "Motion" is another company's brand (motion.dev).

**Working answer:**
- The repository is `lefinehq/motion`.
- The npm package is `@lefine/motion`. The `@lefine` scope already exists, so the name is safe.
- No Rust crates are published in v1.

The brand can be revisited before 1.0.

### Q2. Which 3D

D1 covers 2.5D in v1: card flips, perspective, camera moves, parallax. Is that the 3D the maintainers have in mind for v1, or is true 3D (models, lights) needed earlier?

### D53. A `.mogrt` imports as its project, with its controls as slots

- **Decision:** `motion import template.mogrt` opens the archive and reads the project inside it with the `.aep` reader (D30), then imports the composition the template was made from. Each control of the template's panel becomes a slot of the scene ([D44](#d44-slots-are-named-values-that-layers-follow)), typed by the control and labelled with its name, and the property the control drives follows the slot as `"$id"`. A control that drives an expression control is the slot the import already makes from that control's reads, now named by the panel. Slots keep the panel's order, and a group's name prefixes the labels inside it. A template that holds no project, as one made in an editing app does, is refused with that reason.
- **Why:**
  - The panel is the designer's own list of what may change, so it is a better slot surface than anything the import can infer, and roles, locks and batches work on it unchanged.
  - The archive only wraps a project the reader already reads, so a second reader would duplicate it.
  - A control names its target by composition, layer and property path, ids the reader already keeps, so the binding needs no guessing.
- **Not now:** templates without a project; the font, size and style a text control may also offer; responsive timing and pinning; writing a `.mogrt` back, which waits until `motion export` has a reason to patch an archive.

### D54. A collapsed composition draws into the canvas it sits in

- **Decision:** a `comp` layer with `"collapse":true` draws its composition's layers straight into the canvas it sits in, each under the layer's transform on top of its own, with the layer's opacity and matte. Nothing is clipped to the composition's canvas, and its background is not drawn. The layer's effects run over the canvas it sits in, so their points and lengths are in that canvas's coordinates. Without effects, and without an adjusting layer inside, the layers are not isolated, so their blend modes reach what is below.
- **Why:**
  - Imported projects use it widely: 66 of 193 corpus samples have a collapsed composition, and its layers often reach outside the composition's canvas, which the old clipped drawing cut off.
  - Its effects are measured in the parent's space in the project, which is why the same ramp copied onto a collapsed title and onto a shape lines up there. Measuring them in the composition's own space put the title's ramp out of place.
  - One flag on the existing `comp` layer keeps addresses, overrides and slots working unchanged; flattening the layers into the parent at import would lose the composition as a unit to edit.
- **Not now:** 3D layers inside a collapsed composition sharing the parent's camera and lights. Masks on the collapsed layer wait for masks.

### D55. Masks are items that cut a layer's content before its effects

- **Decision:** a layer's `mask` is a list of mask items, as `fx` is a list of effects. Each mask is a closed path in the layer's own px, `{"mask":"M0,0 L200,0 L200,100 Z"}`, with animatable `feather`, `opacity` and `expand`, and the settings `mode` and `keep` (`inside` or `outside`). The masks combine in order into one coverage:
  - `add` (the default) joins;
  - `subtract` removes;
  - `intersect` keeps the overlap;
  - `lighten` and `darken` keep the larger or smaller coverage;
  - `difference` keeps where one covers and the other does not;
  - `none` is skipped.

  The layer starts uncovered when its first mask adds, and covered otherwise. The coverage cuts what the layer draws before its effects and its matte apply.
- **Why:**
  - An item list reuses what effects already have: keys per property, tracks, edits by address (`title.mask[0].feather`), and checks per kind.
  - The modes, their order and the starting coverage are the ones the projects the import reads use, so a mask imports as one item, with no rewriting.
  - Cutting before the effects lets a glow or a blur spread past the mask's edge, as it does in those projects.
- **Not now:** mask motion blur, linear feather falloff, and masks on shape groups.

### D56. Audio layers, mixed at 48 kHz and read by effects

- **Decision:** a scene plays sound files through `audio` layers, which draw nothing and play in their own time as video layers do. A `volume` gain, a property like `opacity`, fades them, and a `comp` layer's `volume` scales its composition's sound. The engine mixes every audible layer into one stereo track at 48 kHz in 32-bit float, which renders write to the output. Effects read an audio layer as `displacement-map` reads a layer, and properties may follow a sound's level through a modifier.
  - **Reading.** `motion-audio` reads WAV itself, and MP3, FLAC and Ogg Vorbis through pure Rust decoders under permissive licences (`nanomp3`, `claxon`, `lewton`). The format comes from a file's first bytes. AAC comes from the operating system, as H.264 does ([D13](#d13-codec-licence-policy)).
  - **Layout.** A file is decoded once per session, whole, folded down to stereo by its speakers (ITU-R BS.775) and resampled to 48 kHz. Resampling uses our own Kaiser-windowed sinc kernel, which also plays a stretched layer slower or faster.
- **Why:**
  - Every film the product makes has music. Templates also draw from their music, and 9 corpus samples hold spectrum effects drawn as nothing.
  - Decoding whole files keeps any frame computable alone: a spectrum at frame 900 needs the samples around it, not a playhead. A three-minute song at 48 kHz stereo is 66 MB of floats.
  - One kernel of our own gives the same samples on every machine and every run, so mixes can be compared in tests. A library's SIMD paths differ by CPU. With weights per phase, a three-minute 44.1 kHz song resamples in 0.18 s.
  - The permissive pure Rust decoders keep the licence clean, as for codecs. The widest Rust audio library is MPL-2.0, which the dependency policy does not allow.
- **Not now:** panning, audio effects (equaliser, compression), and more than two output channels. See the [plan](../plans/2026-11-20-audio.md).

### D57. Motion blur samples a layer's motion over the shutter

- **Decision:** a layer with `"mblur":true` is drawn as the average of its drawings at several moments while the shutter is open. Its composition's `shutter` sets three things:
  - the angle, as a share of a frame in degrees (180 is half a frame);
  - the phase, where the shutter opens relative to the frame, in degrees;
  - the samples per frame.

  The default is `"shutter":"180 -90 16"`. Each sample is the layer's whole drawing, content, effects and parents included, at that moment. The samples are averaged and then blend, matte and fade as one layer. Whether the layer draws is decided at the frame: a moment before or after its span still moves it, so it doesn't fade in or out at its ends. A layer that moves farther than one sample spacing in px takes more samples, up to eight times the setting. A `comp` layer, or one with effects, is drawn once at the frame and moved to where each moment puts it, up to twice the setting; what moves inside a composition blurs by that composition's own shutter. A `shutter` angle of 0 turns motion blur off for the composition.
- **Why:**
  - These are the settings the projects the import reads keep, as the same three numbers, so an import carries them as they are.
  - Averaging whole drawings blurs everything that moves: transforms, shapes, text and effects. That is what true motion blur means in D21, and what a cheaper streak of the transform alone would miss.
  - More samples for faster layers, as the original's adaptive limit does, keeps fast layers smooth without paying for slow ones.
  - Drawing a composition or an effect stack once per frame keeps the cost of nested compositions and heavy effects from multiplying by the samples at each level.
- **Not now:** frame blending, and motion blur of masks separately from their layer.

### D58. 2.5D layers are planes projected through a camera

- **Decision:** a layer with `"3d":true` is a flat plane in its composition's 3D space. It has depth, X and Y rotation and an orientation besides the 2D transform, and is seen through the composition's camera: the topmost `camera` layer shown, or a default with a 39.6° view. Its content, masks and effects are drawn flat in its own coordinates. The result is then laid onto the canvas through a projective matrix, sampled trilinearly from a mip chain. Pixels whose ray meets the plane behind the camera are left out. Adjacent 3D layers draw back to front: where two show over each other, one that lies wholly behind the other's plane draws first, and of two in one plane the first in the list; otherwise, and to break a cycle, the one whose centre is deeper in camera space. A flat layer between them splits them into separate runs. Orientation keys interpolate spherically. The matrices come from `glam`.
- **Why:**
  - It is the model of D1's first step, and the one the projects the import reads use, so their 3D layers and cameras carry over as data.
  - Drawing in the layer's own space before projecting keeps effects, masks and text exactly as they are for flat layers. Only the last step, laying the buffer down, changes.
  - Inverse mapping with a behind-camera test cuts planes that cross the camera correctly without clipping polygons. Mip-mapped sampling keeps planes seen edge-on from aliasing.
  - Depth sorting by layer, with flat layers splitting the runs, is the order the classic renderer of those projects uses. Splitting planes that intersect would need geometry, and no sample needs it yet.
  - A tested maths crate for 4×4 matrices and quaternions beats our own.
- **Not now:** lights, shadows, materials, extrusion, depth of field, intersecting planes and per-character 3D text.

### D59. Craft lint measures motion from the scene and reports numbers

- **Decision:** `check` gains craft rules: judder, constant speed, text hold and crowding. They are computed from the evaluated scene, frame by frame, without rendering. Each finding is one line with the rule, the layer, the frames, the measured value and its limit, and the edit that clears it when one edit can. Craft rules warn. Two gates, text contrast and flashing, need pixels, so they run on a reduced render through the engine, and they are errors. All limits live in one table. A rule ships only when the planted-defect scenes trip it and few of the corpus's professional templates do.
- **Why:**
  - An agent reads a 50-token report, not frames. A number and a limit tell it how far to move, which a picture does not.
  - These are the layer-2 rules of [Quality](quality.md#craft-lint), with the evidence for each. They measure craft, not taste, which stays with people ([D22](#d22-no-ai-judge)).
  - Computing from the scene is exact and costs milliseconds, so it can run after every edit. Only contrast and flashing depend on what is drawn.
  - The corpus is 198 templates made by professionals. A rule they trip often is a wrong rule, so it is the calibration set for false alarms.
- **Not now:** layout and grid composition, colour harmony, loudness, and anything that would score taste.

### D60. H.264 footage decodes through the system's decoder

- **Decision:** `motion-video` decodes H.264 clips (`avc1`, `avc3`) through the operating system's decoder: on macOS, a VideoToolbox decompression session in `motion-videotoolbox` (`Decoder`, `decoder.rs`), behind the crate's `Decoder` trait ([D47](#d47-video-layers-play-their-clip-in-their-own-time)). Elsewhere a clip in H.264 stays an error that says so, and the import reports it.
  - **Demux** stays ours: the sample table gives each sample's bytes and times (decode times, composition offsets, the edit list). A track without an edit list starts at its first shown frame.
  - **Decode** is the system's, one sample at a time in decode order, synchronously, with no reordering asked for: each sample gives back its own picture, in 4:2:0 at 8 or 10 bits.
  - **Colour** is ours, on every platform: the matrix and range come from the sample description's `colr` box, then the stream's video signal type, and an untagged clip is BT.709 limited range above standard definition and BT.601 at it. Chroma is upsampled linearly from where the stream says it sits. Primaries and transfer are read but not converted.
  - **Boundaries.** Only `motion-video` and `motion-encode` depend on the binding crate, each behind its own codec trait; nothing that draws a scene links `motion-encode` ([D46](#d46-video-is-encoded-in-a-crate-of-its-own-behind-one-codec-trait)), and a boundary test holds both.
  - **Caching.** A clip keeps its last 16 pictures. A picture becomes RGBA only when a frame asks for it, outside the clip's lock, so render workers convert frames side by side while samples decode one at a time. Decoding starts from the last sync sample after which nothing shows before it (open groups of pictures), and a sample that would leave the kept pictures before the asked one is not decoded when no other picture refers to it (`nal_ref_idc` 0).
- **Why:**
  - It is what [D13](#d13-codec-licence-policy) allows: the operating system's decoder, which runs on the media engine where the Mac has one and whose licence covers the codec. Nothing patent-encumbered is compiled in.
  - The binding hands back planes as coded and knows nothing of colour tags, so the colour rules are one piece of portable, tested code. A decoder for another platform, such as Cisco's OpenH264 loaded at run time, only has to hand back planes the same way.
  - Converting only the frames asked for keeps a seek's cost in decoding, which the system does in about 2 ms a 1080p frame, and not in conversion, which takes longer.
  - The fidelity references are untagged HD H.264, and read right only as BT.709 limited range ([fidelity notes](../research/2026-11-14-fidelity-first-sample.md)); footage made the same way is read the same way.
- **Not now:** HEVC and ProRes, which the same session decodes once their configuration and colour are read; Windows, Linux and OpenH264; colour management of primaries and transfer (BT.2020, HDR).

### D61. A font is found by the names its face gives itself

- **Decision:** before an import substitutes a font it was not given ([D40](#d40-missing-fonts-are-substituted-at-import)), it looks for a face that answers to the font's name by the names the face's own `name` table gives it.
  - A face answers to its PostScript name (name ID 6), its full name (4), and its family with its style, both the typographic pair (16, 17) and the legacy one (1, 2), where a `Regular` style is left out. A named instance answers to its family with its subfamily. Names are compared in lower case, without spaces or punctuation.
  - A face with the exact PostScript name wins; else the first face added that answers. A name of another style never answers, so `BebasNeueBold` is still a substitute when only `Bebas Neue Regular` is there.
  - The scene names the face by its own PostScript name. The import's report lists each such font under `renamed_fonts`, and does not count it as unsupported. The fetch of `motion import --fetch-fonts` leaves such fonts out.
  - A render still finds a face only by its exact PostScript name, or by family, weight and slant ([D33](#d33-text-is-shaped-from-font-assets)).
  - The typesetter registers each face of a font collection (`.ttc`) alone, as a copy of the collection whose header lists only that face, made the first time text uses it.
- **Why:**
  - Families rename faces between builds while the face stays the one the project means. Open Sans 1 calls its 800 weight `OpenSans-Extrabold` and Open Sans 3 `OpenSans-ExtraBold`. IBM's own releases of IBM Plex Sans call the regular `IBMPlexSans`, and the Google Fonts build calls it `IBMPlexSans-Regular`.
  - The face's own names say which names it goes by. A hand-kept list of aliases would only grow, and it would still miss the next family.
  - Counting such fonts as substitutes hid fonts that were there. In the local corpus, 8 samples reported 17 uses this way, and one was blocked by it alone. Their text lies against the stored layout exactly as before, since the same file set it, so the rename is the face the project meant. Whether it is the version the project used is the lock's concern ([D39](#d39-fonts-are-resolved-before-a-render)).
  - Registering a whole collection put all its faces under one face's family, so every face after the first was set in the collection's first face, or failed on a glyph that face lacks. No face after the first of a `.ttc` could set text. Copying a collection only for the faces used keeps a large collection cheap.
- **Not now:** matching by name in `motion fonts fetch` (it still picks files by the PostScript name), names a face does not give itself (a family renamed, as Muli became Mulish, or styles renamed by a build, as Google Fonts' Lato 2 calls Lato's Hairline `Thin`), and names in languages other than English (a face's English names are read, else its first ones).

### D62. A layer may remap the time of its content

- **Decision:** a `comp` or video layer has a `remap` property: the moment of its composition or clip it shows, in seconds, at each moment of its own time. It is a number track like any other, keyed in the layer's own time, so it holds, plays faster or slower, and runs backwards by its keys. Without it, the content plays the layer's own time, as before. Only what the layer shows follows it: the layer's transform, effects and span keep the layer's own time. A remapped moment outside the content's length shows nothing. A remapped composition's sound plays the remapped moment at the rate the remap moves.
- **Why:**
  - It is how the projects the import reads hold a sequence: a duration-control rig remaps a nested composition to hold its middle and to run its outro at markers. 26 corpus samples remap a layer's time, and 9 of the 10 closest to passing among those with gaps report it. Before, the import kept the content playing its own time, so an outro ran at the wrong moment or never.
  - A property, not a new kind of layer or a list of segments, keeps everything a track can do: keys, eases, holds, listening, and the expression bake, which writes what a remap expression gives as keys.
  - The content's time is evaluated where the layer's own time already is, so nested compositions, clips and their sound read one value. Nothing else moves.
- **Not now:** frame blending between the content's frames, and remapping an audio layer by itself.

### D63. A range selector eases its choice by a cubic

- **Decision:** a `range` selector has `ease-high` and `ease-low`, from -1 to 1, default 0. The choice its shape gives a part, from 0 to 1, passes through a cubic from (0, 0) to (1, 1) before `amount` scales it: its first handle is (`ease-low`, 0) when `ease-low` is positive and (0, -`ease-low`) when it is negative, and its second is (1 - `ease-high`, 1) when `ease-high` is positive and (1, 1 + `ease-high`) when it is negative. The import maps `ADBE Text Levels Max Ease` to `ease-high` and `ADBE Text Levels Min Ease` to `ease-low`, in percent.
- **Why:**
  - It is how 21 corpus samples shape their reveals, and both open renderers that reproduce the original's text animators read the two values this way. Measured against references at the pixels the ease moves, the cubic errs about a tenth as much as no ease and as the two values swapped ([evidence T1](aep-evidence.md#text-selectors)).
  - A cubic on the choice, not a new shape, keeps every shape easable, as in the original, and keeps the shapes' own definitions.
- **Not now:** the order of ease and `smooth` on a `square` shape is taken from one of the two renderers (ease after smoothness); no sample tells them apart.

### D64. A selector may choose each part by its own amount

- **Decision:** a text animator may hold a `parts` selector. Its key lists `part` items, one per part it counts by `by`, in reading order; each item's key is how strongly its part is chosen, from -1 to 1, and is a track like any other. Two numbers choose a part across and down apart, as the original's three-axis amount does: `position`, `anchor` and `scale` take each axis its own amount, every other value the first, and a range selector's choice is the same both ways. A part past the last item is not chosen. It combines with the selectors before it by `mode`. The import writes one for an expression selector: it evaluates the selector's amount once per part of the layer's text, with `textIndex` (from 1) and `textTotal` set, at every frame its layer shows, and fits each part's values into keys. An amount without an expression gives every part its own value.
- **Why:**
  - An expression selector computes each part's choice by a script ([D10](#d10-modifiers-instead-of-scripts) keeps scripts out of scenes), so what the scene can hold is what the script gave. 16 corpus samples use one, nearly all for a per-character or per-line delayed bounce (`t = time - inPoint - delay·textIndex`), which no `range` shape draws. Some choose a part differently across and down (a squash, or a line that fades while it stays in place), which one amount cannot hold.
  - A list of tracks keeps every part editable, keyed and eased like any other value. The alternative, writing one `range` selector per part, cannot stand for an expression selector in front of others under `intersect` or `subtract`, and hides that the parts belong together.
  - The parts are the scene's own counts, so the count the bake uses is the count the engine draws: characters other than line breaks, those other than spaces, words, and the lines the project last laid the text out in.
- **Not now:** an expression selector after other selectors, whose choice it replaces and reads as `selectorValue`: the import bakes one that comes first, as in 14 of the 16 samples, and reports the others (m619, m809), which a `replace` mode would hold. Depth, which text does not draw yet. Text that changes over time, whose parts are reported.
### D65. A merge replaces the outlines above it by one

- **Decision:** a `merge` shape item is a modifier that takes every outline listed before it in its group, those inside earlier groups included, seen through their transforms, and puts one outline in their place, in its group's space. `merge` mode lays them one after another; `add`, `subtract`, `intersect` and `exclude` combine the areas they cover, each read by the nonzero rule, the first outline's with each next one's in turn, into simple closed contours. The paints listed before a merge draw nothing of what it takes; the paints and modifiers after it see the one outline. The mode is a setting, not a property.
- **Why:**
  - It is what the original does, and the corpus says so where it can tell (evidence M1 to M4 in [what we know about `.aep`](aep-evidence.md#merging-outlines)): 14 samples merge outlines, and a matte drawn by one, m712's, only matches its reference when the fills above the merge draw nothing.
  - A merge is one more modifier in the walk that trims already use: it rewrites the outlines it reaches, so a paint never needs to know a merge was there, and trims before and after it compose by list order.
  - Combining areas needs boolean operations on curves. `linesweeper`, built on the same `kurbo` paths the scene already uses, keeps cubic pieces of the inputs rather than flattening them, and reads each operand with its own fill rule; it sits in a module of its own (`motion-scene`'s `boolean`), for later modifiers that need areas too.
  - A setting, because the original's mode is a menu: no project in the corpus animates it, and an animated menu holds its choice between keys anyway.
- **Not now:** a fill rule per operand other than nonzero, which no project in the corpus needs to tell apart.
Status: Draft · 2026-11-16

### D66. Round corners cut sharp corners back along their sides

- **Decision:** a `round-corners` shape item is a modifier with one property, `radius`. Like a trim, it changes in place every outline listed before it in its group, including those of earlier groups, each in its own space. Every paint that draws an outline draws it rounded. A sharp corner is a vertex with no handle on either side. It is cut back along each side by `radius`, or by half the way to the next vertex when that is shorter. The two cut points are joined by a cubic whose handles reach 0.5519 of the cut back towards the corner. Smooth vertices and the ends of an open outline stay as they are. `radius` rests at 10.
- **Why:**
  - It is the original's modifier, and the corpus confirms its default and its cut (evidence R1 and R2 in [what we know about `.aep`](aep-evidence.md#round-corners)). Rounding at the outline level, rather than as a stroke join, rounds fills too, and makes a trim after it follow the rounded line.
  - It is one more row in the modifier table merges introduced ([D65](#d65-a-merge-replaces-the-outlines-above-it-by-one)), so nothing that draws needs to know about it.
  - A rectangle's own `round` stays, because that rounding is part of the shape. Round corners rounds whatever outlines reach it.
- **Not now:** an arc of `radius` at corners that do not meet square. Every corner measured in the corpus is a right angle, where the two rules agree.
Status: Draft · 2026-11-16

### D67. A repeater copies the items above it as groups

- **Decision:** a shape item `repeater` draws everything listed before it in its group again, `copies` times: as if those items were one group per copy, each under the copy's transform and opacity. Copy `k` stands `k + offset` steps along, each step a move by `position` and a turn and scale about `anchor`; the steps add up (moves by `t · position`, turns by `t · rotate`, scales by `scale` to the power `t`) rather than compound. Its opacity runs from `start-opacity` towards `end-opacity` by `k / copies`. A fractional count draws one copy more, faded by the fraction. `composite` says whether each copy draws under the one before (`below`, the default) or over it (`above`).
- **Why:**
  - It is the one shape operator the original has that neither changes outlines in place nor paints them: it multiplies paints as well as outlines. A new role, not a modifier, says so, and the walk that resolves shapes handles it by walking the items before it once per copy, so every rule a group follows (paints in their group's space, opacity on the paints inside, trims inside before trims outside) holds for copies without a rule of its own.
  - The paints and trims after a repeater reach every copy's outlines as they reach a group's, so a stroke below a repeater strokes all the copies, and two repeaters in a row make a grid.
  - 13 corpus samples use it, 65 repeaters in all, for rings of strokes, grids of dots and trails. The beliefs, checked against their references, are in [aep-evidence.md](aep-evidence.md#repeaters).
- **Not now:** a blend mode per copy, and copies that compound, each step turning the moves after it: no sample tells the two apart.
