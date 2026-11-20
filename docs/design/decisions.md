Status: Draft · 2026-11-13

# Decisions

This is the record of every design decision: what we chose, and why. Other documents link here instead of repeating the reasoning. To change a decision, add a new entry that supersedes the old one. Do not edit the old entry.

The project was named motion until 2026-11-17 and is now nakkash ([D73](#d73-the-project-is-named-nakkash)). The entries before D73 keep the old name, crate names (`motion-format`) and commands (`motion render`) as they were written; read them as nakkash's.

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

- **Decision:** every crate takes the workspace's lints, which forbid unsafe code, except `motion-videotoolbox`, which calls macOS's VideoToolbox, CoreMedia and CoreVideo through their generated `objc2` bindings. That crate denies unsafe operations inside unsafe functions, and every unsafe block carries a `SAFETY` comment (Clippy's `undocumented_unsafe_blocks` and `multiple_unsafe_ops_per_block` are errors). Its API is safe and names none of the frameworks' types, and `motion-encode` puts it behind its codec trait ([D46](#d46-video-is-encoded-in-a-crate-of-its-own-behind-one-codec-trait)). A test fails when any other crate leaves the workspace's lints. A binding for another platform (Media Foundation, VA-API) or for the system's decoders ([D47](#d47-video-layers-play-their-clip-in-their-own-time)) gets the same treatment: one crate per platform, small, with each block explained. `nakkash-machine` is the second such crate: the C library's calls that say what memory the machine has, how pressed it is and what the process used ([D90](#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget)), behind a safe API.
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

  The default is `"shutter":"180 -90 16"`. Each sample is the layer's whole drawing, content, effects and parents included, at that moment. The samples are averaged and then blend, matte and fade as one layer. Whether the layer draws is decided at the frame: a moment before or after its span still moves it, so it doesn't fade in or out at its ends. A layer that moves farther than one sample spacing in px takes more samples, up to eight times the setting. A `comp` layer, or one with effects, is drawn once at the frame and moved to where each moment puts it, up to twice the setting; what moves inside a composition blurs by that composition's own shutter. A layer with an effect whose drawing runs on with time by itself, such as particles, is drawn at each moment instead, effects and all, also up to twice the setting. A `shutter` angle of 0 turns motion blur off for the composition. A pivot draws nothing, so its `mblur` blurs every layer it carries.
- **Why:**
  - These are the settings the projects the import reads keep, as the same three numbers, so an import carries them as they are.
  - Averaging whole drawings blurs everything that moves: transforms, shapes, text and effects. That is what true motion blur means in D21, and what a cheaper streak of the transform alone would miss.
  - More samples for faster layers, as the original's adaptive limit does, keeps fast layers smooth without paying for slow ones.
  - Drawing a composition or an effect stack once per frame keeps the cost of nested compositions and heavy effects from multiplying by the samples at each level.
  - Particles move inside their effect, not with their layer, so a drawing moved as a whole shows them standing still: `mblur` on a still particles layer changed no pixel. Drawing it at each moment pairs each moment's particles with its transform and parameters, so particles, a moving producer and a moving layer blur together. Which effects run on with time is data on the effect's kind (`timed`), not a name the engine knows. Rendering the effect at the shutter's moments inside one buffer would skip redrawing the content, but it needs each moment's parameters and transform anyway; the drawing at each moment costs about 13 ms more a moment on a 1080p layer, all of it the canvas, not the particles.
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

### D68. An expansion moves outlines out by an amount

- **Decision:** a shape layer has an `expand` item, a modifier like `trim`: it moves every outline listed before it in its group, those in earlier groups included, `amount` px to the left of the way it runs, so outward for the clockwise outlines `rect`, `ellipse` and `star` draw. Each side moves along its normal, a curve to its true parallel within 0.001 px. Sides that part are joined by `join` (`miter` within `miter` amounts of the corner, `round`, `bevel`); sides that cross are cut where they cross. An open outline becomes the closed outline around it. `copies` repeats the outline one `amount` further out each, shifted by `copy-offset` amounts.
- **Why:**
  - Boxes behind titles are drawn this way: a rectangle sized to the text, grown by a margin. 44 expansions in 10 corpus samples grow rectangles, most of them sized to a text by an expression, by 10 to 72 px a side.
  - A modifier, not a grown `rect`, because the outlines it grows can be any of a group's, paths and stars included, and because the outline itself changes: every paint that draws it, and every trim after it, sees the grown one, as with a trim.
  - Moving each side to its exact parallel and joining or cutting at the corners keeps the outline's own structure, one outline in, one out, which trims and fill rules after it count on. Growing by a stroke and a union of areas would merge outlines and lose their direction.
  - The miter limit counts in amounts from the corner, as a stroke's counts in widths: m624's bar, grown 28 with the limit 4, has square corners in its reference, where a limit in px would bevel them.
- **Not now:** removing the loops a large inward amount leaves where sides other than neighbours cross; they are filled by the fill's rule as drawn.

### D69. A reference frame passes over its codec floor

- **Decision:** a sample in the fidelity manifest may name a floor file, `floor`, which gives each reference frame its codec floor: the share of its content that fails the tolerance only because the reference is a lossy encode. The file also gives each frame's error floor: the mean content error the encode alone causes. A frame passes when its failing share is at most its floor plus `DiffOptions::PASSING` (1%) and its mean content error at most its error floor plus `DiffOptions::ERROR_MARGIN` (0.004 on the 0 to 1 scale); a floor file without error floors holds the failing share alone. A sample passes, every frame or once settled, as before, a window still needing a frame with content (`Rule::Floor`). The strict rule, 1% whatever the floor, is kept (`Rule::Strict`): `motion fidelity` prints the score under the floor rule first and under the strict rule second, and the ledger records, per window, the frames passing under each rule, the mean floor and the mean error floor. A sample without a floor has a floor of 0, under which both rules agree, and a ledger written before floors reads as measured that way. The floor is measured outside motion, by the local corpus tooling: motion's own render of the sample, encoded with the reference's own encoder settings at its size and frame rate, decoded as the reference was, and compared with the scene.
- **Why:**
  - The corpus references are frames of x264 encodes (CRF 23, the settings x264 writes into each stream), so a render identical to the original's fails some of their pixels: under the strict rule, frames of samples drawn right failed on the encode alone (m650's frames fail on up to 1.40%, of which its encode alone accounts for 0.96%). The limit is meant for what motion draws, not for what the reference lost.
  - The floor is per frame because the encode's loss is: flat frames lose almost nothing, detailed and moving ones the most (m759's floors run from 0% to 0.75%).
  - Our render encoded once is compared with our render, not with the reference: compared with the reference, two encodes' losses add up. Measured on our render, the floor depends on the build that drew it, so the tooling records the build and measures again when the scene or the build changes.
  - The failing share alone is not enough. On the local corpus the floors' median is 6.22% and 19 are above 25%: where the content is small saturated colour (4:2:0 halves its chroma) or photographic, nearly every pixel fails on the encode alone, and the failing share no longer tells a right drawing from a wrong one. m806, a photograph, fails on 78.2% against a floor of 80.0% and passed every frame on that alone, with a mean error of 0.0618 against the encode's 0.0538. The mean error still tells them apart.
  - The margin is the smallest that keeps the samples drawn right passing: m759, m650, m562 and m783, whose frames lie at most 0.0002, 0.0038, 0.0029 and 0.0016 above their error floors. The floors are measured with another x264 build than the references' (core 165 against 161), which also leaves their failing-share floors 0.1 to 0.5 points under what those references fail. Twice the margin, 0.008, would also pass m560 and m573 on every frame and m752 and m806 once settled.
  - Keeping the strict figure keeps the bar the quality document sets in view, and keeps a floor that is too generous from hiding.
  - Encoding stays in the tooling, so motion needs no encoder for it ([D13](#d13-codec-licence-policy)); motion reads numbers.
- **Not now:** a floor for references that are not lossy, which need none; the encoder build the references used is not ours (x264 core 161 against 165 here), so a floor is an estimate, which the margins absorb.

### D70. Vector footage is drawn from its outlines

- **Decision:**
  - Footage whose file is PDF, which includes every `.ai` file saved with PDF content, is drawn by motion itself, from its outlines: nothing is converted to pixels, or to another format, outside the engine. A new crate, `motion-vector`, reads the file's first page into the rasteriser's own display list ([D34](#d34-a-layer-tree-renderer-on-vello_cpu)), and our rasteriser draws it.
  - **One drawing model for every vector format.** A reader interprets its file and paints on a `Canvas`: `fill` and `stroke` (outlines given in their own coordinates with the transform to the page; even and uneven scales handled once, for every format), `picture`, `push_clip`, `push_group` (opacity, blend mode, soft mask) and `pop`, and `unsupported` for what it meets and cannot draw. The canvas turns these into nodes. `Drawing::new(format, size, art, canvas)` makes the result, in the page's space: its units, y down, origin at the page's top-left corner. Formats are rows of `FORMATS`, each a name, a test of the file's first bytes (`HEAD`, 1024 of them) and a reader. The PostScript reader planned as D71 adds a row and paints on the same canvas; the engine and the import then need no change.
  - **PDF.** The file's objects are read by `hayro-syntax` and its drawing operators interpreted by `hayro-interpret` (0.9, MIT or Apache-2.0, pure Rust; the interpreter forbids unsafe code), through a device of ours that paints on the canvas. The interpreter keeps the graphics state, `q`/`Q`, transforms and clips, reads the external graphics states (constant alpha, blend modes, soft masks, line styles), and resolves colour spaces, shadings, patterns, Form XObjects, transparency groups, images (Flate, DCT, and the other standard filters) and embedded fonts. Our device maps each paint: solid colours; axial and radial shadings as gradients, a new paint of the rasteriser, whose stops follow the shading's function to within 0.01 of a channel; shadings a gradient cannot draw exactly (an end that does not extend, a background, function and mesh shadings) as a picture sampled at 4 pixels a point, at most 2048 a side, clipped to the outline; tiling patterns as their cell's operators run again at each step, as vectors, up to 4096 cells; soft masks as mattes (luminosity over the mask's backdrop colour, or alpha); text as the outlines of its embedded glyphs, Type 3 glyphs as their operators; image masks in the fill colour.
  - **Colour** is converted to sRGB as it is read, by the interpreter: device grey and RGB as they are, device CMYK through the interpreter's built-in press profile, ICC-based colour through the file's own profile (to its alternate space when the profile cannot be read), and the other spaces through their definitions. It then blends in gamma-encoded sRGB, as every layer does.
  - **Rasterising.** By default a vector layer is drawn once into a picture its box's size, one pixel per unit of the box, with the portable rasteriser so it is the same everywhere, and that picture is transformed like any picture (`Drawing::picture`, kept per size). With the layer's switch that collapses a composition's transformations, the same switch in the project, the import writes `"continuous": true`, and the vectors are laid over the box through the layer's transform and drawn at the output's resolution every frame (`Drawing::placed`).
  - **Scene format.** The image layer keeps `image` and `box`, the format recognised by the file's first bytes, so a scene written before reads as it did. Two keys are new on image layers: `crop`, the part of the picture the box shows (`[x, y, width, height]` in the picture's own units, pixels or points; it also crops raster pictures), and `continuous`. Both are additions, so the version stays 1.
  - **Which part of the page.** The import picks the page box whose size, rounded out to whole units, is the footage's size in the project: the crop box (written as no `crop` when it is that size exactly), else the art box. A file whose size matches neither is drawn page over box, and reported.
  - **What is not drawn is said.** The device notes fonts the file does not embed, images that cannot be decoded, soft-mask transfer functions, image masks painted with a pattern and over-large tiling patterns; a scan of the file's dictionaries, indirect or written inside others, notes overprinting, transfer functions and knockout groups. The engine's footage finder reads each vector file it finds, and the import reports each note on every layer that shows it (`vector footage: …`), through its usual report. A file that cannot be read is reported by its format.
  - **Toolchain.** `hayro-interpret` 0.9 needs Rust 1.92, so the toolchain moves from 1.91 to 1.92.
- **Why:**
  - Eleven `.ai` files in ten corpus samples are all PDF inside (`%PDF-1.5`, `%PDF-1.6`), and the import drew them as nothing. Drawn by motion, m563 goes from 2 to 8 of 13 frames passing over its codec floor and from 0.0289 to 0.0208 mean error, m564 from 1 to 6 of 11 and from 0.0154 to 0.0075, m593 from 1 to 3 of 20, m562 from 0.0022 to 0.0012, and m830 from 0.0460 to 0.0307: as far as pictures of the pages drawn by another renderer took them, and further for m830, whose art box an outside picture of the page missed ([evidence](aep-evidence.md#vector-footage)).
  - **A drawing model of our own, not the interpreter's device.** The display list is what every layer draws through, so a vector layer gets effects, masks, mattes, 3D and motion blur with no code of its own, and stays sharp under any transform when it is continuous. Our own canvas, not the PDF device trait, is the interface, so a reader of another format depends on motion's types alone ([D37](#d37-engines-stay-behind-our-own-types)); a boundary test keeps the PDF crates out of every public API.
  - **The parser.** Writing a PDF object parser and content interpreter ourselves would repeat years of work on broken files, fonts, colour and filters. Of the pure-Rust candidates:
    - `hayro-syntax` and `hayro-interpret` are maintained (0.9 in November 2026), interpret the whole imaging model behind a device trait, and were written for this kind of rasteriser. They read all eleven corpus files without a warning. *Chosen.*
    - `lopdf` reads and writes objects and splits content streams into operators, but interprets nothing: the graphics state, colour spaces, shadings, fonts and images would all be ours to write. *Rejected.*
    - `pdf` reads objects and operators, with the same gap, and has had no release since March 2026. *Rejected.*
    - Bindings to renderers written in C would add C dependencies, and some carry copyleft licences. *Rejected.*
    - The same project's own renderer draws into a pixel buffer of its own, so its output could not join the display list or stay vector. *Rejected.*
  - New dependencies: the hayro crates (PDF syntax, interpreter, fonts, character maps, a PostScript scanner, and decoders for JPEG 2000, JBIG2 and CCITT images), `moxcms` and `pxfm` for ICC colour (BSD-3-Clause or Apache-2.0), and `phf`. The embedded fonts and character maps the interpreter can bundle are left out: text in a font the file does not embed is reported, not guessed.
  - **Box size by default, continuous with the switch.** That is how the original treats vector footage, and the corpus agrees where it can tell: m803 scales its ornament 5.3 and 3.4 times with the switch on, and drawn continuously it errs less than drawn at box size on 18 of its 20 frames and as much on 2 (0.0924 against 0.0929). The samples that shrink their logo cannot tell the two apart.
  - **The art box when the footage's size says so.** m830's footage is 444 by 662 on a 1920 by 1080 page, its art box rounded out; with the art box it errs 0.0307, with the page stretched over the box 0.0440, and drawn as nothing 0.0460.
  - **The portable rasteriser for the picture of a vector.** It is drawn once per size, so its cost hardly counts, and the same bits on every machine keep renders repeatable ([D12](#d12-what-repeatable-means)).
- **For the reader of another format:** add a row to `motion_vector::FORMATS` (name, first-bytes test, reader); the reader paints on a `Canvas` in the page's space and returns `Drawing::new`. Report what it cannot draw with `Canvas::unsupported`, in the same words where the meaning is the same. Nothing in the engine, the import or the scene format changes.
- **Not now:**
  - Pages after the first: every corpus file has one page.
  - Overprinting, transfer functions and knockout groups are drawn without, and reported.
  - Shadings that are not gradients are pictures, so a continuous layer scaled past 4 pixels a point softens them; images ignore their `Interpolate` flag and are sampled as pictures are; a line of width 0 is drawn one point wide.
  - The luminosity of a soft mask is the matte's, not the PDF's own weights; no corpus file has a soft mask.
Status: Draft · 2026-11-17

### D71. EPS footage is drawn by our own PostScript interpreter

- **Decision:** an EPS file is footage motion draws itself. `motion-postscript` runs the PostScript program inside it, a plain file or the program a binary EPS header points to, and returns what it paints as vector items over the file's whole-point `%%BoundingBox` (read from the trailer when the head defers it with `(atend)`), with the precise `%%HiResBoundingBox` as where the art lies: fills in page space with their fill rule, strokes in their own user space with their pen and the matrix to the page, pictures with the matrix from their samples to the page, each with an sRGB colour and an index into a tree of clips. Page space starts at the box's top-left corner, y down, one unit a point. The interpreter has the language's scanner, operand, dictionary, execution and graphics-state stacks, control and composite-object operators, files and the decoding filters (hexadecimal, base 85, run length, deflate, sub-file), the path, clip, paint, colour and image operators, and as much of the second language level as EPS writers' procedure sets use: resources with implementation dictionaries a program may copy, `<< >>`, `setcolorspace` with indexed, separation and n-channel spaces through their tint transforms, image dictionaries. Device CMYK becomes sRGB by the language's own rule, each primary `1 − min(1, c + k)`. Whatever a program uses that is not drawn (text, smooth shading, patterns, user paths, masked images, encrypted sections, binary tokens) is a note on the page, which the import reports on each layer showing the file. A run is bounded by `Limits` (100 million operations, 64 Mi memory units, an execution stack of 4096, nested runs of 32, output and image limits) and ends with a note, keeping what it drew, when the program errs without catching it or reaches a limit.
- **Why:**
  - The footage must draw inside the engine, at any size, with no conversion outside it. An EPS file is a program, not a data format: its drawing is whatever its procedure set computes. The corpus's binary EPS files hold half a megabyte of procedure sets before a few hundred path operators, and draw nothing unless those sets run: they define resource categories by copying `Generic`, read their own metadata through a sub-file filter, ask for system parameters and the language level, and choose their colour operators by what they find. Matching the drawing operators of known writers would draw these five files and no others.
  - The interpreter executes from an explicit stack of frames, never Rust recursion, so `exit`, `stop` and the error handlers are truncations of a vector, a procedure that ends by calling itself runs in constant space, and the depth limit is one comparison. Composite objects live in tables that one run owns and drops, so dictionaries that hold themselves leak nothing.
  - The private data some writers append after `%%EOF` is never read: `%%EOF` ends the program, as the document-structure conventions define it, and a binary section a program declares but does not read is stepped over, not scanned. Encrypted sections are skipped to their end, never decoded.
  - The page is the whole-point box because that is the footage's size in every project that holds EPS footage, and the art sits in it at one point a pixel, not stretched to it: m794's 77.3357 by 23.0674 point art is footage of 78 by 24, and drawn over the whole-point box its content error is 0.01919 against 0.01961 stretched from the precise box ([evidence E2](aep-evidence.md#eps-footage)).
  - The CMYK rule is the one the reference renders show. m772's footage is `0 .993347 1 0` CMYK: the reference draws it (255, 3, 2) and (255, 0, 0), the rule gives (255, 2, 0), and the system's generic CMYK profile would give (217, 37, 42) ([evidence E1](aep-evidence.md#eps-footage)). PDF footage goes through a press profile instead ([D70](#d70-vector-footage-is-drawn-from-its-outlines)); none of the corpus's 16 PDF and `.ai` files paints in CMYK, so no reference tells which the original uses there.
  - The output is our own vector vocabulary on `kurbo`, not the rasteriser's, so the interpreter knows nothing of drawing and the vector footage path ([D70](#d70-vector-footage-is-drawn-from-its-outlines)) lays it on its canvas like any other format's.
  - No new dependency: `kurbo`, `indexmap` and `flate2` are already the workspace's.
- **Not now:** text (fonts are found and scaled so procedure sets run, but `show` and its kin draw nothing and are reported), smooth shading and patterns, user paths, masked images, the LZW, DCT and fax filters, binary tokens, and memory rolled back by `restore` (it brings back the graphics state only). The interpreter answers to language level 2, so procedure sets take their level 2 paths, which the corpus's draw with plain paths and fills.

### D72. Scenes written from scratch get choreography as data and a guide

- **Decision:** the engine gives an agent writing from a brief the moves a motion designer reaches for as short data, and the repository gives it a guide and examples ([plan](../plans/2026-11-23-choreography.md)):
  - `out`, an exit that mirrors `in` and ends with the layer's span;
  - preset starts measured in the canvas's size, and built-in presets for the common entrances: `rise`, `slam`, the four `push` directions, `swing-in`;
  - examples written from scratch, without brand, footage or real names, each passing `check` in CI;
  - a guide for agents, also shipped as an agent skill, that keeps the check's measured limits apart from practice that is still provisional.
- **Why:**
  - Real use writes the same moves by hand in every film. About forty 9:16 films a studio wrote with the CLI spell out every exit and every push as keys per layer, with pivots parked off the canvas before and after. One key on each side of a push says the same thing, and the check can then tell a push from a layer left off the canvas.
  - An agent copies what it is shown. Examples that pass the check, and a guide with numbers, cost tokens once and save them in every scene; templates teach the template's own habits instead.
  - The practice in the guide comes from an AI critic scoring frame sheets, whose scores moved more between batches than the effects it measured. It stays practice, labelled as such, and no check rule is built on it ([D22](#d22-no-ai-judge)).
  - It is measured the way the brief set was built to measure it: `motion bench` before and after, with the same agent, model and prompt, and people watching both sets.
- **Not now:** new check rules from that practice, copywriting, and beats as a layer kind: `labels` already name moments.

### D73. The project is named nakkash

- **Decision:** from 2026-11-17 the project, its crates, its command and its scene format are named **nakkash**, the maintainers' choice. A nakkash (Turkish *nakkaş*) was one of the classical miniature painters, the artists who illuminated manuscripts.
  - **Renamed:** the crates `motion-*` → `nakkash-*` and their Rust paths `motion_*` → `nakkash_*` (so log filters and targets name `nakkash_*`, and the timings target is `nakkash::timings`); the command `motion` → `nakkash`; the scene key `"motion": 1` → `"nakkash": 1`; `MOTION_LOG` → `NAKKASH_LOG`; `MOTION_CACHE` → `NAKKASH_CACHE`; the font cache's folder (`~/Library/Caches/nakkash`, `%LOCALAPPDATA%\nakkash`, `$XDG_CACHE_HOME/nakkash`); `MOTION_BRIEF` → `NAKKASH_BRIEF`; the test-only variables (`NAKKASH_BLESS`, `NAKKASH_AEP_SAMPLES`, `NAKKASH_OUT`, `NAKKASH_FONTS`, `NAKKASH_VIDEO_CLIP`, `NAKKASH_EXPR_SOURCES`, `NAKKASH_EXPR_TRACE`); the font fetcher's user agent; the handler name in MP4 files; the agent skill's folder, `skills/nakkash`; and the repository, `lefinehq/nakkash`.
  - **The scene key.** New scenes, imports and examples write `"nakkash": 1`; nothing writes `"motion"`. The reader takes `"motion": 1` as the same key in the same place, so the scene is the same scene, and reports a deprecation note. A scene with both keys is an error at `motion`. An unknown-key error never offers `motion`. An edit keeps the key the file has: writing back keeps what the author wrote. The old key is read for all of format version 1, and the migration from version 1 to any later version reads it too, so a scene written before the rename stays readable ([D36](#d36-the-format-only-changes-in-the-open)).
  - **Other aliases, until 1.0:** the `motion` command, a second binary of `nakkash-cli` that runs the `nakkash` beside it; `MOTION_LOG`, read when `NAKKASH_LOG` is not set; `MOTION_CACHE`, read when `NAKKASH_CACHE` is not set; the font cache's old folder, read for the files the new one lacks, each copied into the new folder as it is read; and `MOTION_BRIEF`, which `bench` still sets for its agents. The test-only variables have no alias.
  - **Every alias says so.** Each use logs a `warn` under the target `nakkash::deprecated`, which the command shows whatever its log level, in the chosen log format. The note names the new name and this decision.
  - **History stays as written.** The changelog's earlier entries, D1 to D72, plans and research notes keep the old name, with a note at the top of the changelog, of this record and of the documentation index. Living documents (the README, the guide, the scene format and the other design documents) use the new name.
- **Why:**
  - "motion" is also the word for what the engine draws. In our own documents and code the product could not be told apart from motion graphics, motion blur or a layer's motion, nor searched for.
  - Scenes outlive the code that wrote them ([D36](#d36-the-format-only-changes-in-the-open)): the fidelity corpus, other projects' films and agents' prompts hold `"motion": 1`. Reading the old key costs one alias, and a test that reads every corpus scene under both keys and finds the same scene; rejecting it would break all of them at once.
  - Commands, variables and cache folders live in scripts and on machines we cannot see. An alias with a note keeps them working and tells their owners what to change; 1.0 is when the aliases go, since a release is when users expect names to settle.
- **Not now:** rewriting `"motion"` to `"nakkash"` when a scene is edited, and a command that migrates scene files: either would change files that tools still running the old command must read.

### D74. An opt-in GPU path approximates the CPU reference

- **Decision:** `nakkash render --gpu` hands the float canvas's work to a GPU through `wgpu`, and nothing else does. The CPU path stays the reference and the default. This supersedes the part of [D3](#d3-cpu-first-compositor) that kept all compositing on the CPU; D3's reasons still decide everything else ([evidence](../research/2026-11-18-gpu-backend.md)).
  - **Where it may be used.** Only `render`, and only when asked: previews, drafts, look development. Tests, golden images, `check`, `at`, `diff` and fidelity, `bench`, `batch` and `export` draw on the CPU, and `diff` draws its frames on the CPU even in a session handed a GPU. Delivered files are drawn without `--gpu`. The engine's `gpu` feature, which only the CLI turns on, is the one way to reach the GPU crate, `nakkash-gpu`.
  - **It says so.** A render with `--gpu` writes a note on stderr before it draws, naming the adapter and saying that its frames approximate the reference, and a tally after: the effects the GPU ran, by name and count; the effects it left to the CPU for want of a kernel, by name and count; its other operations; and the grids copied each way, with their bytes.
  - **Only hardware.** It asks Metal, Vulkan or DX12 for a high-performance adapter. An adapter that reports itself as a CPU, or as of no known type, is refused, as is no adapter at all: the command warns with the reason and draws on the CPU. The GL backend is not compiled in, since it is how software rasterisers are usually offered. Our Linux render servers have no GPU, so `--gpu` there is a warning and a CPU render.
  - **What the GPU does.** The rasteriser still draws every path, glyph and picture on the CPU. The GPU takes what the float canvas does with their pixels: laying a layer by any of the blend modes, with its opacity, clip, masks and matte; combining a layer's masks (each mask's outline is drawn by the rasteriser, then grown, feathered and combined on the GPU); mixing filter layers; moving pixels; laying projected (2.5D) layers through their landings ([D58](#d58-25d-layers-are-planes-projected-through-a-camera)); and every effect with a plan. That covers the blurs (`gaussian-blur`, `box-blur`, `radial-blur`, `vector-blur`, `lens-blur`), `glow` by any of its operations, `drop-shadow`, `bevel` and `solid-composite`. It covers the colour and channel effects (`curves`, `levels`, `tint`, `fill`, `hue-saturation`, `exposure`, `black-white`, `invert`, `sharpen`, `find-edges`, `set-matte`, `shift-channels`, `color-cycle`) and the distortions (`displacement-map`, `turbulent-displace`, `polar-coordinates`, `lens-distortion`, `repeat-tile`, `motion-tile`, `wrap`, `mirror`, `bezier-warp`). And it covers the generators (`fractal-noise`, `ramp`, `four-color`, `beam`, `grid`, `venetian-blinds`, `linear-wipe`, `iris-wipe`).
  - **Plans.** An effect's plan is what its parameters come to on one layer's pixels, lengths in pixels of the grid and choices made, a row of `nakkash_raster::PLANS` by effect name. The CPU function of each of those effects runs its own plan, so the GPU reads parameters exactly as the reference does and never reads them itself. An effect gains a GPU kernel by gaining a plan row; the effect registry does not change for it. Giving an effect a plan leaves its CPU output the same bits.
  - **Where pixels are.** A frame keeps its float canvas on the GPU from the start when it has work there: an effect the GPU runs, in any layer, layer read or matte source, or a projected layer. Every layer's pixels then go to the GPU once they are drawn. What the rasteriser drew goes up as its 8-bit bytes and is widened there through a table of the CPU's own `c / 255`. An operation on pixels on the GPU runs there, and a drawing that an effect reads, or that several layers share, stays where it was drawn. Pixels come back to the CPU only for a step without a kernel, and then go up again with the next step that has one. The frame comes back once, at the end, packed on the GPU into the 8-bit bytes it is written in. A frame with nothing for the GPU is drawn on the CPU, to the bit, and the GPU is not touched: deciding by what the frame holds, not by a timing, keeps the choice the same on every run.
  - **One submission.** Every thread records into one command encoder, and every copy from the CPU is a command in it, through a staging belt. So the GPU runs work in the order it was recorded, and a pooled buffer can be taken again as soon as it is let go of. The encoder is submitted when pixels must come back to the CPU, or once 256 operations or 512 MiB of copies are recorded, so the GPU starts on a long frame while the CPU still draws. A frame whose pixels stay on the GPU is one submission.
  - **What stays on the CPU.** Effects without a plan: `particles`, `contour-strokes`, `audio-spectrum` and `audio-waveform`. Within effects with one: a `set-matte` read off a colour without its alpha, or off its hue or saturation (next to a transparent pixel the CPU's last bit decides between 0 and 1), swirly block noise, and a radial blur centred more than 2^24 px (zoom) or 2^15 px (spin) from the layer's origin. Also motion blur of a drawing, a clip's coverage (exact 8-bit coverage, one multiply, laid on the GPU as values), and grids, or a plane's halved copies, larger than the adapter's largest storage binding. Each costs one copy each way at its boundary and is listed in the tally.
  - **Pixels.** A grid on the GPU is a storage buffer of `vec4<f32>`, premultiplied, row by row: the layout of the CPU's `Pixels`, so a copy either way is the bytes as they are.
  - **Tolerances.** Each kernel is held to a limit on the largest difference from the CPU, of any channel of any pixel, on values from 0 to 1. A harness runs the kernel and the CPU function it ports on the same pixels (`crates/nakkash-gpu/tests/parity/`), and whole frames of the examples are held to one 8-bit level (`crates/nakkash-cli/tests/gpu.rs`). Both skip, saying so, where there is no hardware adapter. An effect that reads another layer is checked twice, with that layer's drawing on the CPU and held on the GPU. The measured column is the largest difference over the harness's cases, on an M4 Max through Metal:

    | What runs on the GPU | Limit | Measured |
    |---|---|---|
    | Copies up and down; 8-bit bytes widened; cuts by values | 0 (bit for bit) | 0 |
    | Mixing a filter layer | 1e-6 | 6.0e-8 |
    | Laying, every blend mode, with clips, masks and mattes | 1e-5 | 5.7e-7 |
    | Layer masks: grown, shrunk, feathered, every mode | 1e-5 | 4.2e-7 |
    | Moving pixels (bilinear resampling), moving effects | 1e-4 | 8.6e-6 |
    | Projected layers, from bytes, floats or held, to a 4K far corner | 1e-4 | 1.6e-5 |
    | The same, and a hair off whole px, by a corner pin's box filter ([D89](#d89-a-corner-pin-lays-its-layers-plane-through-a-homography)) | 1e-4 | 2.1e-5 |
    | A chain of 700 lays, blurs and mixes recorded at once, from four threads | 5e-5 | 3.6e-6 |
    | `gaussian-blur`, `box-blur` | 1e-5 | 4.2e-7 |
    | `glow`, by every operation; by dissolve | 1e-5; 1e-4 | 7.2e-7; 3.6e-7 |
    | `drop-shadow` | 1e-4 | 5.4e-7 |
    | `solid-composite` | 5e-5 | 9.5e-6 |
    | `radial-blur`, zoom and spin | 3e-6 | 6.6e-7 |
    | `lens-blur` | 2e-5 | 5.0e-6 |
    | `bevel` | 5e-6 | 1.4e-6 |
    | `vector-blur`; with its map softened | 2e-4; 5e-3 | 6.0e-5; 1.6e-3 |
    | `curves`, `levels`, `tint`, `fill`, `hue-saturation`, `black-white`, `invert`, `sharpen`, `find-edges`, `shift-channels` | 1e-6 | 1.8e-7 |
    | `exposure`, `color-cycle` | 1e-5 | 3.2e-6 |
    | `set-matte` | 1e-4 | 3.9e-5 |
    | `displacement-map`; with its map read straight | 3e-4; 2e-2 | 8.3e-5; 4.9e-3 |
    | `turbulent-displace`, `lens-distortion`, `motion-tile` | 3e-4 | 7.9e-5 |
    | `polar-coordinates`, `mirror`, `bezier-warp`, `repeat-tile`, `wrap` | 1e-4 | 2.9e-5 |
    | A distortion 3,000 px from the origin | 1e-3 | 3.3e-4 |
    | `fractal-noise`; 3,000 px from the origin | 1e-4; 3e-4 | 3.6e-5; 8.5e-5 |
    | `four-color`, `beam` | 1e-4 | 2.4e-5 |
    | `grid` | 1e-5 | 2.4e-6 |
    | `ramp`, `venetian-blinds`, `linear-wipe`, `iris-wipe` | 1e-6 | 1.8e-7 |
    | A whole frame of an example, as 8-bit channels | 1 level | 1 level, in at most 66 of 8.3 million channels |

    The loose rows divide by something small. A straight reading divides a colour by its alpha, so near a transparent pixel it carries a float's rounding of the alpha; a vector blur's streak takes a whole number of samples from a length; a softened vector map is such a reading, sloped and then divided by its steepest slope.

    A long chain of effects carries each kernel's difference on into the next, and the loose ones (straight readings of a map, a softened vector map) can make it larger. A 1080p frame of a commercial template, with 389 effects on the GPU, is the CPU's in 99.35% of its 8.3 million channels; 46,187 differ by 1 level, 5,186 by 2 and 2,282 by 3 to 18. That is a preview's difference, not a delivered frame's, which is why delivered frames are drawn without `--gpu`.

- **Why:**
  - Heavy effects are where the CPU path is slowest. On one CPU thread a 4K Gaussian of blurriness 50 takes 156 ms and a glow of radius 80 179 ms; the GPU runs them in 9.4 and 12.9 ms. A 1080p frame of a commercial template, with 5,339 effect runs, takes 434 s on one thread. A frame gains only what its time in those kernels is, so a light frame gains nothing and is drawn no slower: the GPU is not touched.
  - On the CPU path a frame is bit-identical for the same scene, version, OS and CPU type ([D12](#d12-what-repeatable-means)), which D12 already says a GPU cannot promise. A GPU's result depends on its driver: Metal compiles kernels with fast math, which may fuse and reorder arithmetic, and other drivers round otherwise. So the GPU cannot be the reference, and every place that measures or delivers stays on the CPU.
  - A software adapter would be slower than the CPU path and still not the reference. Using one silently on a server would make renders slower and different with no one asking.
  - 32-bit floats, not 16. The CPU's canvas is 32-bit floats. A 16-bit float keeps 11 bits of precision, a step of 4.9e-4 near 1, an eighth of an 8-bit level each time it is stored; a Gaussian stores six passes and a glow eight, so the errors would add to a level and more, and the harness would have to allow 1e-3 everywhere, where it now catches a wrong kernel at 1e-5. A 4K grid is 127 MiB rather than 63, and copies need no conversion either way.
  - Storage buffers, not textures. Kernels port the CPU's own sampling and edges, so they need no filtering; buffers keep the CPU's layout; per-pixel kernels change them in place; and a 4K grid fits the smallest binding an adapter may limit to, 128 MiB.
  - The box passes port the CPU's sliding window: each invocation slides over one stretch of one line, the stretch about as long as the window once the window is long, so a pass costs the same at any radius. The window's sum is a 96-bit integer in units of 2^-64, where the CPU's is an f64. A running sum in 32-bit floats keeps the rounding of values that have left the window: beyond a blur's reach, where the CPU's sum is exactly 0, it left noise near 1e-11 of either sign. An effect that reads straight colour turned that noise into any colour at all, and a displacement map read from a blurred layer was 178 levels off in a small frame. An integer sum is exact whatever Metal's fast math reorders, and costs nothing measurable: the pass is bound by memory.
  - The frame stays on the GPU because copies, not kernels, were the cost. Every copy back waits for all the work recorded before it. On a heavy commercial-template frame, with each effect without a kernel copying its layer each way, `--gpu` was no faster than the CPU (11.8 against 11.7 s). With every family ported and the frame resident, the same frame runs 389 effects on the GPU and 2 on the CPU, comes back to the CPU once, and takes 1.6 s ([evidence](../research/2026-11-18-gpu-backend.md#the-resident-frame-measured)).
- **Rejected:** choosing CPU or GPU per frame by a timing guess: which path drew a frame would then depend on the machine's load, so the same render could give different frames on the same machine.
- **Not now:**
  - Drawing paths and glyphs on the GPU. With the rest of a heavy frame on the GPU, the rasteriser is the largest share of its time: 49% of one heavy frame and 33% of another. An opt-in path through `vello` behind D2's interface is written up in the research note; it is not built.
  - Kernels for `particles`, `contour-strokes` and the audio effects, and motion blur on the GPU.
  - `--gpu` for `batch`.
  - Deterministic kernels (fast math off), which would make the GPU repeatable on one machine but still not the reference.

### D75. A frame is drawn on every thread of its render

- **Decision:** a render draws on one pool of threads, as many as `--threads` gives (one per processor without it), and both its frames and the rows of each frame share them.
  - **Frames.** Each thread of the pool takes the next frame while frames are left. Frames are not tasks of the pool: a thread that waits for the rows of its frame works rows, of its frame or another's, and never starts a frame on top of the one it is in.
  - **Rows.** Inside a frame, the work over pixels (effects, laying layers on the float canvas, masks, mattes, clips, motion blur of a drawing, laying planes) is split into runs of whole rows, about 16 thousand pixels each, that any thread of the pool takes. A thread with no frame left works the rows of the frames still drawn, so a render of one frame, such as `--frame N`, is drawn on every thread, and so is the end of a long one.
  - **The same bits.** Each task writes only its own pixels and reads what is shared; each pixel is worked out by the same code in the same order of operations as on one thread, and no task sums or keeps anything across another's pixels. What tasks make once and share, such as the halved copies a plane is sampled from, is made by one of them, alone, and is the same whoever makes it. A frame is the same bits on one thread and on many, so [D12](#d12-what-repeatable-means) still holds as it is: the same scene, version, OS and CPU type draw the same frames, whatever `--threads` is or how busy the machine is. A test applies every effect on one thread and on six and compares the bits; the golden images, drawn before this change on one thread, are drawn on every thread now.
  - **Adopting it.** An effect's loop over pixels becomes one of the rasteriser's row helpers (`par::each`, `par::each_with`, `par::map`, `par::rows`, `par::columns` for passes down columns, `par::bands` for tiles), and its body stays as it was. A body that adds into pixels from many sources keeps the sources' order within each pixel, so it splits by the pixels it writes, never by the sources.
- **Why:**
  - Frames were drawn one per thread, so a frame was drawn on one core: a single frame of a heavy 4K template took minutes while the other cores waited, and so did previews and `--frame N`.
  - One pool for both levels keeps the limit a limit: frames times rows never pass it, since they are the same threads. A second pool for rows, or threads started per effect, would pass it as soon as two frames ran at once.
  - Keeping frames out of the pool's tasks keeps memory and order where they were: a thread that took another frame while waiting would hold two frames, and the frame under it, which the in-order hand-over to the encoder waits for, would wait for the one above it.
  - Rows, not tiles or layers, because nearly every pixel loop already walks rows, and a split by rows changes the loop and not its body: the effects still open as pull requests adopt it in a few lines each.
  - The rasteriser's own threads are not used: `vello_cpu` makes a pool of its own for each render context, outside the limit, and a frame makes a context for every layer drawn alone.
- **Not now:** drawing sibling layers that are each drawn alone at the same time and laying them in order, which needs the painter's state split and the per-frame cache of drawn inputs shared between threads; filling paths on several threads; and sums over a frame's pixels done by many threads, whose order would change their last bits.

### D76. A composition is drawn at the size it lands at

- **Decision:** a composition, and any layer, is drawn at the size it lands at on the canvas, never at its own size and then shrunk: its layers are drawn under the transforms that place them, on canvases of the frame's size, and the lengths and points of their effects reach the pixels through those transforms ([D49](#d49-effects-that-read-pixels-run-on-a-canvas-of-their-own)). A 4K composition placed at half size in a 1080p frame is drawn at 1080p. One case is drawn whole, and only for [`Quality::Precise`](../../crates/nakkash-raster/src/render.rs), the delivered frames:
  - **A shrunk layer whose effects read only a few px where it lands.** When what holds a layer shrinks it, every effect on its pixels reads only within a reach (none reads its whole box or beyond), and one of them reads less than 8 px of the canvas round each pixel, the layer is drawn at its own size and laid down through its transform, as a plane is ([D58](#d58-25d-layers-are-planes-projected-through-a-camera)): its buffer starts on a whole px of the canvas, so the halved copies it is sampled from are the means of whole blocks of its px. A kernel of a px or two (`sharpen`, edge finders) or a box blur of a few px takes another shape on the coarser grid, and nothing else does.
  - **`Quality::Fast`** draws every layer where it lands, for previews.
- **Why:**
  - It is how the renderer already draws (D49). The question was whether that is exact, so it was measured: the scene drawn through a composition at half and a quarter of its size, against the same scene drawn whole and shrunk by the means of 2×2 and 4×4 px, by the measure and limits of `nakkash diff` ([D69](#d69-a-reference-frame-passes-over-its-codec-floor)): a pixel fails beyond 0.016, a frame passes with at most 1 % failing and a mean error within 0.004.
  - Where it lands is within those limits for the examples and for effects with a wide reach. The four `examples/*.json` and `examples/advanced/ui-reveal.json` at half size, every frame: at most 0.79 % failing, a mean error of at most 0.0003. A gaussian blur, a drop shadow and a glow whose reach stays wide where they land: at most 0.22 % failing at half size. Drawing whole costs what drawing at the larger size costs: drawn whole, the examples took 2.7 to 3.7 times the CPU time they take at half size (title-card: 43.1 s against 11.7 s for its 120 frames).
  - Where an effect reads a few px it is not exact, and drawing whole is. A box blur of 4 px on a bar: 8.74 % failing at half size and 13.40 % at a quarter, 0.00 % drawn whole; `sharpen` at 50: 3.02 % and 6.08 %, 0.00 % drawn whole; a glow of radius 20 at a quarter: 1.68 %, 0.00 % drawn whole. A test in `nakkash-engine` holds a scene of these effects to the limits at half and a quarter of its size.
  - Effects that read their whole box are kept out of it. Drawn whole, they would read all of the box where at the size the layer lands they read the part the canvas shows, so drawing whole would change what they draw, not only how finely. On a 4K template's frame placed at 1080p, drawing those whole too changed 84 % of the pixels. Which part of its box such an effect reads is a question of its own (a buffer the size of its box), not of the size it is drawn at.
  - The 4K template frame, against the same frame drawn at 4K and halved, fails on 44.19 % with a mean error of 0.0519 where it lands, and on 43.81 % with 0.0482 with the rule, for 16 % more time (92.2 s against 107.0 s on 14 threads). What remains there is not the size: effects that read their whole box see the part the canvas shows, and noise and thresholds that amplify any difference.
- **Not now:** drawing a whole composition at its own size when its blend modes would mix differently there; a reach for each effect measured in px of the canvas rather than of the layer, which would let `sharpen` say so itself (its row now reads 1 px of the layer); and a limit by the noise generators' finest detail, which at the size they land can fall under a px.

### D77. Every imported template names what a designer would vary

- **Decision:** `nakkash import` gives every `.aep` and `.mogrt` a complete set of slots ([D44](#d44-slots-are-named-values-that-layers-follow)), each with a role, so a variant of any template is a list of slot values ([plan](../plans/2026-11-24-template-slots.md)):
  - **Text.** Every text layer's words follow a `text` slot; copies of one text stacked in one composition, in the same font, follow the same slot. The role (`title`, `subtitle`, `body`, `label`, `url`, `email`, `phone`, `date`, `price`, `cta`, `number`, `text`) is read from the layer's name, its words, its size against the other texts of its composition, and its composition's name. The layer gets a `fit`, so longer words stay where the designer laid out the shorter ones: point text shrinks to the size its lines covered (`"fit":"shrink 820x140"`), point text of several lines wraps at that width first (`"fit":"wrap 820x140"`), and box text shrinks into its box (`"fit":"shrink"`). A text never grows.
  - **Colour.** Every colour the scene draws with (fills and strokes of layers, shapes and text runs, text animators, effect parameters, each key of a keyed colour, and `bg`) follows one entry of a palette of `color` slots. Colours within 0.04 of each other in OKLab are one entry, held by the most used. A use that is not its entry exactly keeps its difference in the reference, in OKLCH and alpha, `"$accent l-0.021 c+0.004 h+1.6"` or `"$accent / 0.5"`, so the template draws as before and a new value of `accent` moves every use with it. The keys of a keyed colour that a bake wrote frame by frame follow the nearest entry, whatever the distance. A colour control that expressions read is already a slot, and becomes the entry of its colour. Entries are named by what they paint: `background`, `text`, then `accent`, `accent-2`… for colours and `light`, `dark` and `grey` for neutrals; the label gives the hue and the value.
  - **Media.** Every footage file a layer draws, still, vector or clip, is an `image` or `video` slot, which every layer drawing that file follows, with the role `logo`, `image`, `video` or `texture`. The layer gets `"fit":"contain"`, so a new file of another shape keeps its proportions inside the box. Footage the import cannot draw (a file it cannot find, a codec without a decoder) is a slot whose value is `null` and draws nothing, as it does now, until a variant gives it a file.
  - **Timing.** The scene's `dur` follows a `duration` slot, and the markers of each composition become its `labels`, so a variant can see the template's sections and retime them.
  - **Controls.** Expression controls and a template's panel stay slots as D44 and D53 make them, under their own names; the import adds to them and never renames them.
  - **Edit compositions.** A composition whose name says it is for editing (`EDIT`, `CHANGE`, `REPLACE`, `YOUR`, `PLACE`, `INSERT`, `CUSTOMIZE`) is the strongest evidence of a placeholder: footage in it is a `logo` or an `image`, never a `texture`, and its texts and files are listed first. A composition named for a logo makes the footage in it a `logo`.
  - **Declaration.** A slot declaration may carry `role` beside `label`: `{"text":"SPEED","label":"SAMPLE in 01_EDIT_TEXT_and_LOGO","role":"title"}`. Ids are the role, numbered in reading order (`title`, `title-2`), so the same kind of template gets the same ids.
  - **Locks.** The import writes no locks. D51's defaults (`@logo`, `palette`, `typefaces`) belong to a brand's master, and a template's logo, palette and type are what its user replaces; the designer locks them when the template becomes a master.
  - **Export.** An export compares the edited and original scenes with their references resolved, so a slot's value reaches every value that follows it, and each goes back where the project stores it through the marks D45 keeps: a new `accent` is written into every fill, stroke and effect colour that follows it. A slot's own declaration goes back only where it stands for a control (D44). A text, a `fit` and a file are reported as not written, with why: the project keeps a text's laid-out lines beside its words, which the export cannot lay out again as the project's own tool would.
  - **Live slots.** `nakkash slots --verify` changes each slot alone, draws it at half size at the frames where what follows it shows, and reports every slot that leaves them unchanged; then changes all of them at once and checks the scene. A slot that changes nothing is a bug of the import. `nakkash slots --coverage` counts the text layers, colours and footage no slot holds.
- **Why:**
  - Before this, 95 of the 198 corpus templates imported with no slot at all, and 12 of their 998 text layers followed one. A template sold for rebranding, with compositions named `01_EDIT_TEXT_and_LOGO`, imported with nothing to set, and a new logo meant editing key paths by hand. The engine exists to make better templates, so every one must yield variants.
  - The same few values recur: the corpus draws with a median of 4 distinct colours per template (90th percentile 9, at most 46). A palette of a few named entries is what a designer means by "the colours", and binding every use makes one change of `accent` recolour everything that used it.
  - Keeping a use's difference from its entry, instead of rounding it into the entry, keeps every imported template drawing exactly as before, which fidelity measures ([D35](#d35-fidelity-is-measured-against-reference-renders)), while still linking a template's near-duplicates and its translucent copies of a colour to the one a variant changes.
  - Fitting is what makes text slots usable: a title laid out for `SPEED` overflows its design with `SPEED DIGITAL AGENCY`. Shrinking or wrapping into the size the designer gave the original keeps the composition, and never growing keeps short words as designed.
  - Slots are values ([D44](#d44-slots-are-named-values-that-layers-follow)), so a variant stays a list of typed values an agent can read in one listing, renders stay deterministic, and roles ([D50](#d50-roles-name-what-a-masters-layers-do)), batches ([D52](#d52-a-batch-makes-every-variant-from-one-master)) and overrides work on them unchanged.
  - A slot an agent sets and the frame ignores wastes the variant and the agent's trust, so every slot is rendered once to prove it lives, across the corpus, and the command that does it ships for every template.
- **Not now:** slots of motion (easing, speed, staggers), which wait for the style profile; growing text into free space; retiming that stretches keys inside compositions; writing a text back into its text document, and a file into a footage item; and slots for audio.

### D78. Generated variants move along measured axes and pass when they add no error

- **Decision:** `nakkash vary` makes variants of any scene, imported or written, along six axes, each a function of the master, a recipe and a seed ([plan](../plans/2026-11-25-vary.md)):
  - **palette:** new colours from a hue rotation, an analogous or complementary scheme, or brand colours given as input. Each colour keeps its WCAG relative luminance, so every contrast ratio between two of them stays the master's. A brand colour is exact, and the colours read against it move in lightness until each pair is back at the master's ratio, or at the contrast limit if that is lower. Colours change through the scene's palette slots ([D77](#d77-every-imported-template-names-what-a-designer-would-vary)) where it has them, else by value, so every use of one colour changes together.
  - **type:** fonts paired from the faces the scene can load (its `fonts`, its lock and the cache), classified from their own tables: serif, sans, slab, display, script and mono, with width and x-height. New copy of any length is fitted into the master's text box at rest: set smaller down to a floor, then wrapped into balanced lines, then held longer when the read needs it. A text with a `fit` (D77) is fitted by the renderer instead. Scripts the shaper cannot set, or a font lacks glyphs for, are reported, not drawn.
  - **content:** rows of a CSV or JSON file, each a variant, whose columns name a slot (`$title`), a role (`@headline`) or a layer key (`card-2/title.text`); an image fits inside the box it replaces.
  - **timing:** a new duration, a beat grid (a tempo, or one found in a music track) or a pace (`snappy`, `calm`). It is a time map on the master: moves keep their speed and their easing curves' shapes, and holds take up the difference; labels and the moments where moves start are what snaps to beats.
  - **format:** 9:16, 1:1, 4:5 or any size. What matters (text, logos, other content) is boxed over time from the evaluated scene, grouped, scaled and placed in the new safe area; backgrounds scale to cover, aimed at their busiest part. A centre crop is never the method.
  - **improve:** the craft repairs the check measures: motion blur where it finds judder, a curve where it finds constant speed, lightness where it finds low contrast, a longer hold where it finds a short read. Each repair is kept only if the report after it is no worse on any rule and layer, and better on the one it targets.
- **The master is not rewritten for time or size.** Timing and format turn the master's root into a composition drawn by `comp` layers: a `remap` track is the time map, and each group of what matters is one instance of the master, placed by its own transform, with the layers of the other groups hidden through `overrides` ([D43](#d43-one-layer-address-and-edits-per-instance)). Palette, type and content change values in place.
- **Gating is the baseline the edits use.** A variant passes when its full report, the rules and the pixel gates, has no error that the master's does not already have, measured as bad or worse, with addresses read back through the wrapping. A measure in pixels is read at the master's scale, and the noise of measuring at other moments and sizes is taken off in the variant's favour: 1 px of an overshoot, 0.1 of a contrast ratio, the precision both are printed at, since a copy shows the master's frames between them and the pixel gates draw them at a quarter size. A contrast error the variant adds is repaired before it is gated: the text's fill moves to the lightest or darkest colour of its hue, or of the scene's own colours under a `palette` lock, whichever clears it and adds nothing. The master's report is computed once per run. `batch` gates the same way.
- **A master and a resized copy of it are judged alike as they move.** A layer that moves across the edge for half a second or less, then leaves it, is passing through, whatever its size. Before, only a box wider than the canvas escaped, so a glitch's copies that blow past a 16:9 frame passed and the same copies scaled into 9:16 failed, by amounts that rose and fell with the scale.
- **Locks bound an axis; they never stop it.** Under `palette`, the palette axis reassigns the scene's own colours to one another; under `typefaces`, the type axis varies weight, size, tracking and case inside the master's families. A locked layer keeps every value, including through a slot it follows; a locked slot keeps its value. A variant cannot change `roles` or `locks` ([D51](#d51-locks-are-refused-in-the-edit)).
- **Determinism.** The same master, recipe and seed give byte-identical scenes. Each axis draws from its own generator, seeded from the seed, the variant's index and the axis's name; candidates are sorted before they are drawn from; nothing reads the clock, the order threads finish in, or the network.
- **Output:** for each variant, its scene, recipe, check report, render and a sheet of frames; for the run, one sheet with every variant beside the master, and a report. `--dry-run` stops after the check.
- **Why:**
  - A designer's master is the best starting point there is ([D23](#d23-v1-is-variant-first)), but a variant that can only swap words leaves most of the work to people. Palette, type, time and size are where a campaign's versions differ, so each is an axis the engine can walk on its own.
  - A variant is judged against its master, not against an empty report. Professional templates ship with findings, such as a glitch that throws text past the frame on purpose; refusing every variant for them refuses every variant of that master. The baseline that `check --baseline` and `edit` already use measures exactly what a variant adds.
  - Keeping each colour's luminance keeps its contrast with every other colour at once, so a palette that passed the master's contrast passes the variant's without a search, as long as the colour stays in gamut; the gate catches the rest.
  - Re-placing what matters is what a person does with a reframe. A centre crop of a landscape master keeps its middle 32 %: it cuts any line wider than that, and anything placed towards a side, such as a logo in a corner.
  - Remapping time on a wrapped master keeps every key, curve and baked expression exact, where rewriting every time in the scene would touch thousands of values per template and break what the import cannot re-derive.
  - Improvement is judged by the check's numbers ([D20](#d20-quality-is-measured-in-five-layers)), never by a model looking at frames ([D22](#d22-no-ai-judge)). A repair that clears one finding and makes another worse is not kept.
  - This refines [D26](#d26-reinterpretation-stays-inside-the-masters-style): timing stays inside the master's style, since it only rescales the master's own curves, while palette and type leave it because they are asked to. Locks are where a designer says they may not.
- **Not now:** generated copy, beats chosen by meaning, video and image content recoloured, right-to-left layouts mirrored (text is set right to left; the layout stays), and variants of variants.

### D79. An edit checks only what it can change

- **Decision:** a check is kept, not only reported. `Checked` holds each layer's findings, the frames at which it catches the eye and the glyph boxes of each frame at which it is read, and the runs of text on text. The check after an edit starts from the check before it and reads again only the layers the edit can change; every other layer's findings, and its share of `crowd` and `overlap`, are kept. The scene's rules are judged again over both.
  - **What an edit can change** is found from the two compiled scenes, not from the edit line. A layer, at any depth, is read again when its node, or the node of a `comp` layer around it, differs between the two; when a node it moves with, or its matte, or what those move with and are matted by, does; when a camera of its composition does, or its composition's canvas or clock; when what reads it as a matte or an effect input changes, which keeps a hidden composition evaluated; and when its layer in the scene differs, which fix hints read. Slots, roles, overrides, expressions baked at import and edits of a composition every instance shows all reach the check through the compiled nodes, so they need no rules of their own.
  - **When the shape changes**, a layer added, removed or drawing another composition, or the scene's own canvas or clock, every layer is read again: then the paths themselves differ.
  - **Reading** is the same either way: frames are evaluated side by side on the threads a render may use, and each layer then takes its frames in order. A report is the same on any number of threads, to the bit and in the same order ([D12](#d12-what-repeatable-means)).
  - **The session keeps it.** `Session::check` makes it once per version of the scene; `Session::edit` keeps the version before, so the check after starts from it; `Session::variant` shares both, so each variant of a batch starts from the master's check. `Report::since` compares a report with a baseline in time proportional to their lengths.
  - **Proven against the check from scratch.** A test gives every scene of the compatibility corpus, the examples and the fixtures random edits in runs, of layers at any depth and of the compositions they show, and requires after each that the kept check's report equals a new session's check of the edited scene; a folder of large private scenes can be added to it.
- **Why:**
  - On a 4K template imported with 6,159 layers at any depth (135 instances of one logo composition, each holding 22 texts), one `nakkash edit` took more than ten minutes, almost all of it in comparing the report with the one before, one violation with every other. Agents edited the JSON by hand instead (measured 2026-11-18).
  - An edit changes one layer, or a composition and its instances. Checking everything again costs as much as the first check; checking what changed costs what changed.
  - Finding what changed from the compiled scenes, not the edit line, means there is one place that knows what a node reads, next to the code that evaluates it. Rules that read the edit line would have to know every way a slot, a role or an override reaches a layer, and would drift from the compiler.
- **Not now:** evaluating only the compositions that hold a changed layer (every frame of the scene is still evaluated); keeping checks across runs of the command, in a cache on disk; and the pixel gates, `contrast` and `flash`, which draw the scene and are kept for a later decision.

### D80. Learned models run in a bake, never in a render

- **Decision:** inference runs only in `nakkash ml preview` and `nakkash ml bake`. They write **baked assets**: files kept in a cache by the SHA-256 of their manifest, each with a record of what made it. `render`, `check`, `at`, `diff`, `fidelity`, `batch` and `export` read baked assets and never infer. A scene whose request has no bake, or whose bake was made for another request, is an error that names `nakkash ml bake`.
  - **Boundaries.** Inference lives in two crates: `nakkash-ml` (the model lock, the store, the pipelines) and `nakkash-onnx` (the runtime binding). Only the CLI depends on them, behind its `ml` feature, off by default; what reads baked assets is `nakkash-baked`, which links neither. A boundary test holds this.
  - **Recorded.** Every asset records the hashes of the model files and the runtime library that made it, where each graph ran (D81), the thread count, the platform, the nakkash version and the request. A bake on another machine or backend is a new asset, never a cache hit.
- **Why:**
  - Learned models give different bits on different machines, backends and even thread counts, which a render must not ([D12](#d12-what-repeatable-means)). Measured on our graphs ([evidence](../research/2026-11-19-learned-models.md#determinism)): the Mac's accelerated backend against its CPU differs by up to 2e-3 in logits; the Mac's CPU against the Linux box's CPU differs in every graph; the point tracker's features differ between 2 and 4 threads on one Mac (5e-7). None of it shows in an 8-bit matte, all of it breaks byte goldens. A render that reads the same baked bytes is the same on every machine.
  - The cost is a bake's, not a frame's: 0.95 s a 1080p frame to segment an object on the Mac and 5 to 10 s to refine its edges; on the Linux box, 17 s and about 55 CPU-seconds ([evidence](../research/2026-11-19-learned-models.md#speed)).
  - A frozen result is what a person wants from a matte: it does not change because a model was updated.
- **Not now:** inference inside the GPU preview path ([D74](#d74-an-opt-in-gpu-path-approximates-the-cpu-reference)); inference inside a render by a deterministic pure-Rust kernel path, which none of the runtimes offers.

### D81. One inference runtime, loaded at run time from a locked file

- **Decision:** `nakkash-onnx` runs graphs through `ort`, pinned to `=2.0.0-rc.15` (ONNX Runtime 1.31), with only its `load-dynamic` linking: no build step downloads or links a runtime, so the default build and CI need neither the runtime nor any model.
  - **The library is locked like a model** ([D82](#d82-a-model-lock-and-a-content-addressed-model-store)). It comes from one supplier for both machines: the vendor's own package of the runtime, `Microsoft.ML.OnnxRuntime` 1.31.0 on nuget.org, pinned by the package's SHA-512 (the registry's published hash), and on each platform by the SHA-256 of the library inside it: the macOS arm64 library, which carries the CoreML provider, and the Linux x64 one, which needs glibc 2.28. `nakkash ml models fetch` downloads it into the store; a bake checks its hash before it loads it.
  - **Placement is per graph** and written in the lock. On macOS, a graph whose input shapes are fixed runs on CoreML (ML Program format, all compute units). Every other graph, and every graph elsewhere, runs on the CPU. A CPU session asks for the CPU explicitly, since the runtime otherwise moves it onto CoreML on macOS.
  - **Sessions** take the thread count asked for (`--threads`, 4 by default), one thread between operators, sequential execution, every graph optimisation and deterministic compute.
  - **CoreML's compiled models** are kept under the store, in a folder named by the graph's SHA-256, the provider's options and the runtime's version. The graph is opened from its place in the store, which is named by its hash and never moves, because the provider keys its own cache by path.
- **Why:**
  - It is the one runtime that runs every graph in scope without porting it, and the measured parity of our exports holds through it: the object segmenter's whole video loop matches the reference implementation with a mask IoU of 1.000 on 48 of 48 frames ([evidence](../research/2026-11-19-learned-models.md#parity)).
  - One signed supplier, with a hash the registry publishes independently of us, for both the Mac and the Linux box. The alternatives fail on facts: the runtime's source repository answers 404 (checked 2026-11-18), so it cannot be built from source; the binding's own prebuilt Linux archive needs glibc 2.38 and GCC 13 and does not link on the box's Debian 12; and its static archives are fetched at build time from a CDN with one operator.
  - Fixed-shape graphs gain 9× (segmenter image encoder), 6.5× (point features) and 5.8× (flow) on CoreML. Graphs with dynamic shapes do not compile there (build errors −7 and −14). The edge matting graph gains nothing (5.8 s against 5.4 s on a quiet CPU) for a 5 to 19 minute first compile and 7 GB of compiled cache, so it stays on the CPU ([evidence](../research/2026-11-19-learned-models.md#speed)).
- **Not now:** GPU providers on Linux and Windows, none of which was measured; building the runtime from source while its repository is unavailable; a second runtime as a conformance oracle, whose coverage of the large backbones is unknown.

### D82. A model lock and a content-addressed model store

- **Decision:** `crates/nakkash-ml/models.lock`, built into nakkash, names every model by a key that says what it does (`segment.video`, `matte.edges`, `points.video`, `flow.dense`) and pins every file it needs by SHA-256 and size: graphs, constant tensors and the runtime library. A key groups the graphs of one model and says where each runs (D81). Each file says how it is obtained:
  - `download`: a URL of the upstream's own release;
  - `member`: a file inside a downloaded archive, the archive pinned by its own hash;
  - `export`: a graph we exported from upstream weights, with the weights' URL and hash, the exporter's hash, the toolchain and the parity measured against the upstream implementation. Upstreams publish weights, not these graphs, so they are taken in with `nakkash ml models import <folder>`, which keeps only files whose hash the lock has.
  Files are kept in the store, `ml/files/<sha256>` in the nakkash cache (`$NAKKASH_CACHE` or the platform's cache folder), written whole and read with their hash checked. `nakkash ml models fetch` is the only command of the set that reaches the network; `nakkash ml models verify` hashes the store again. Scenes never name a model, a file or a path.
- **Why:**
  - It is the font lock's design ([D39](#d39-fonts-are-resolved-before-a-render)): the hash is the identity, the name only a pointer, renders and bakes stay offline, and a mirror or a renamed upstream cannot swap the bytes.
  - Pinning an export by its hash makes any exporter's output acceptable exactly when it is the same bytes. Our exports are byte-identical across toolchains where measured (the edge model's normalised graph from two ONNX versions).
- **Not now:** hosting our exported graphs, which is the maintainers' decision; signatures beyond the package registry's; compiling a model ahead of its first bake.

### D83. Only models licensed for commercial use

- **Decision:** every lock entry carries its licence as an SPDX id, the upstream page it was read from (never a mirror), and a class: `ok` (commercial use and changes allowed), `conditions` (allowed with attribution or use terms), `non-commercial` or `copyleft`. The lock is refused when an entry is `non-commercial` or `copyleft`; a `conditions` entry needs its notice in the lock, which `nakkash ml models list` prints. The lock lists the refused models with their reasons, so a later edit does not take one in by mistake.
- **Why:** many of the strongest models are non-commercial or copyleft, and mirrors carry wrong licence tags. Checked upstream on 2026-11-18: the four models in the lock are Apache-2.0, MIT and BSD-3-Clause, weights included; the best video matting and inpainting models and the leading point tracker are non-commercial or GPL ([evidence](../research/2026-11-19-learned-models.md#licences)).
- **Not now:** clearing the terms of the datasets the permissive weights were trained on; tracked, not blocking.

### D84. Models are untrusted input

- **Decision:** nakkash takes ONNX graphs and raw constant tensors only, never a pickled checkpoint. Before a file enters the store, nakkash reads the graph's protobuf itself, with size caps, and refuses one that uses an operator domain other than the standard one (no custom operators), stores data outside the file, or is larger than the format allows (2 GiB). Exports from pickled weights are made outside nakkash, offline, in eval mode, with parity checked against the upstream module; only their ONNX output is imported, by hash. A Rust dependency added for inference is vendored and scanned before it is added.
- **Why:** pickle runs code when loaded; ONNX external data has had path-traversal and link vulnerabilities; custom operator libraries load native code into the session. Reading the graph ourselves before the runtime sees it closes all three, and costs one pass over the file when it enters the store.
- **Not now:** sandboxing the bake process beyond an ordinary user process; model scanners as a gate.

### D85. Baked results enter the scene as requests on video layers

- **Decision:** a video layer asks for what a bake makes, and keeps the digest of what it made.
  - **`cutout`** asks for the layer's own object, cut out of its clip ([D87](#d87-an-objects-matte-is-its-segmentation-refined-at-its-edge)). It holds prompts on frames of the clip (a box, points to include, points to exclude), its settings, and, once baked, `baked`: the digest of the matte. The render multiplies each frame's alpha by the baked alpha of the same clip frame, so the matte follows the clip through `offset`, `stretch` and `remap`.
  - **`tracks`** asks for motion tracks of objects in the clip ([D88](#d88-a-track-is-baked-once-and-applied-as-keys-on-pivots)), each with an `id`, prompts, optional corners, correction anchors and, once baked, `baked`. `nakkash ml track` writes a baked track as keys on pivots; nothing reads tracks at render time.
  - **Assets.** A matte is one 16-bit greyscale PNG per clip frame at the clip's size plus a manifest; a track is a manifest of per-frame values. A manifest names its request, each file's hash and the record of D80. An asset lives in `baked/<digest>/` in the cache, its digest the SHA-256 of its manifest. A render checks the manifest's hash and each frame file's, and that the manifest's request is the layer's request and its clip's hash; anything else is the error of D80.
  - **Work cache.** Each stage of a bake (the segmentation, the edge refinement, the point tracks, the flow) keeps its result under a key over its model files, runtime, placement, threads, canonical settings, prompts and the clip's hash. A preview's segmentation is reused by the bake after it, and a track and a cutout with the same prompt share one segmentation.
- **Why:**
  - The request lives on the layer that plays the clip, so the matte's frames and the clip's frames are the same frames by index, whatever the layer's time does.
  - A digest in the scene freezes the result: a later model, a new nakkash or another machine never changes a matte that was approved, and a changed prompt or clip is caught, not drawn with an old matte.
  - Keeping requests and digests in the scene makes bakes scriptable by agents with the edit commands they already use.
- **Not now:** mattes and tracks of layers other than video layers; matte assets as editable outlines; partial re-bakes from a changed prompt onwards, which need checkpoints of the segmenter's memory.

### D86. Bakes are gated by measured quality on fixed shots

- **Decision:** each baked task has gates on a fixed set of shots with exact ground truth: a synthetic product shot (rendered, with exact alpha, corners, surface points and visibility) and green-screen composites for hair and fur. Mattes are gated on IoU at 0.5, its minimum over frames, SAD per frame and the error of their change between frames (dtSSD), paired so a blurred matte cannot pass; tracks on the median and 90th percentile of their error after an anchor, and the mask centroid's mean error. The limits are the measured values of the pipeline the gates were set on, with a margin, in one table in the gate test. The gates need the models, so they are ignored tests that the bake machines run (`cargo test -p nakkash-onnx --test gates -- --ignored`, with the shot folder in `NAKKASH_ML_SHOTS`; they live with the runtime binding because they run real graphs).
  - **Tolerances across machines:** 8-bit alpha within one level on at most 0.01 % of pixels, track positions within 0.05 px, between any two platforms and backends. Bit-exact only for one platform, backend and thread count, which a double bake on one machine checks.
- **Why:** single temporal measures are gameable: flow smoothing lowered the label-free flicker measure by 20 to 37 % while its error against ground truth rose 7 to 25 %, and on hair it fell below the ground truth's own, which is lag, not stability ([evidence](../research/2026-11-19-learned-models.md#mattes)). Exact synthetic ground truth catches what composites cannot.
- **Not now:** a longer synthetic shot and a real hair shot with a measured matte; tuning the edge band and agreement threshold on a held-out set.

### D87. An object's matte is its segmentation refined at its edge

- **Decision:** a `cutout` is baked in three stages.
  1. **Segment.** The video segmenter (`segment.video`) finds the object from the prompts and propagates it through the clip. Its memory of past frames is kept in Rust, as the reference implementation keeps it: the prompted frames first, then up to six frames before, then up to sixteen object pointers, each memory rounded to bfloat16 when stored. Every prompted frame conditions the whole clip; propagation runs forward from the first prompted frame to the end, then backward from it to the start, both reading one memory. Masks are the 256² logits, upsampled to the clip's size.
  2. **Refine the edge.** The edge matting model (`matte.edges`) runs on a crop around the object (its box over two frames either side, with a fifth of its size and 16 px around it), at 1024². Only the band of 12 px either side of the segmentation's edge takes its alpha, and only on frames where its mask, gated by the segmentation, agrees with the segmentation at an IoU of at least 0.9. Elsewhere the segmentation's probability stays. Band and agreement are settings (`band`, `agree`).
  3. **Smooth**, only when asked (`smooth`): the flow model (`flow.dense`) carries the previous frame's matte onto the current one where the flow is consistent both ways, and blends the two by a weight that falls with their difference.
  `edges` picks `refine` (the three stages, the default), `hard` (the segmentation's mask) or `soft` (its probability).
- **Why:**
  - A salient-object matte run on the whole frame takes whatever stands out: a passing skateboarder instead of the product (IoU 0.11), the hands with the phone, the laptop around its screen. The segmenter never lost the object (worst-frame IoU 0.95 to 0.99 on three shots) but its edges are soft.
  - Refining only the band where the two agree cut hair SAD by 26 % and its dtSSD by 45 %, tied on fur, and costs a little on a hard-edged product (SAD 7.7 against 6.2). When the refiner disagrees, the matte is the segmentation, so it is never worse than the segmentation by more than the band ([evidence](../research/2026-11-19-learned-models.md#mattes)).
  - Cropping raises the refiner's resolution; it does not fix its choice, which is why the band and the agreement test stay.
  - Smoothing by flow lowered flicker only by lagging (above), so it is never the default.
  - Treating every prompt as an initial conditioning frame makes the result a function of the scene, not of the order a person clicked in.
- **Not now:** several objects in one cutout; prompts from text; checkpoints of the memory for partial re-bakes.

### D88. A track is baked once and applied as keys on pivots

- **Decision:** a track request is baked into one asset and written as ordinary keys by `nakkash ml track <scene> <layer>.tracks[<track>] --to <pivot>`, the track named by its address.
  - **What is baked:** the segmentation of D87 for the track's prompts, and from it, per clip frame, the mask's centroid, its area's scale against the prompted frame and its principal angle; up to 64 points chosen inside the mask on the prompted frame (corners by the smallest eigenvalue of the structure tensor, at least 12 px apart at 512²) and followed through the clip by the point tracker (`points.video`) with their visibility; per frame, a similarity fitted to the visible points (least squares on the inliers of a RANSAC homography) and that homography, guarded: it is used only with at least 8 inliers and when no corner lies further than half the scaled diagonal from where the similarity puts it, else the similarity's corners are, and the frame is marked.
  - **Corners** are given on the prompted frame for a screen or a label, or found as the mask's extreme points along its diagonals.
  - **Anchors** correct drift. An anchor gives the corners on a later frame where the object is visible. Points are chosen and tracked again from it, and between two anchors each frame blends the fits from both by its distance to each, so the track meets every anchor exactly.
  - **Keys:** on the pivot, parented to the video layer and given its `offset` and `stretch`, one linear key per clip frame, at the time the layer shows it: `position` and `scale` from the mask (robust under occlusion), `rotate` from the points, and with `--corners`, four corner pivots (`<pivot>-tl`, `-tr`, `-br`, `-bl`) from the guarded homography, which a `corner-pin` reads ([D89](#d89-a-corner-pin-lays-its-layers-plane-through-a-homography)). A video layer with `remap` is refused, since its frames do not map to one time each.
- **Why:**
  - Measured on the synthetic product shot: the mask centroid stays within 1.4 px on average (5.4 px at worst) through a half occlusion; points hold at 1 to 3 px until the object is half covered, then drift to 20 to 37 px; one correction on a visible frame cuts the drift after it from 31 to 2.4 px; unguarded homographies explode (1,291 px), guarded ones hold 4.8 px over the first half ([evidence](../research/2026-11-19-learned-models.md#tracks)).
  - Reducing keys removes almost nothing (32 to 48 of 48 position keys kept at a quarter pixel), so every frame keeps its key and reduction stays a hand edit.
  - Keys on a pivot are the scene's own vocabulary: anything parented to it follows, and the render never reads a track.
  - Parenting the pivot to the video layer puts the keys in the clip's own coordinates, so any move of the video layer carries the track with it.
- **Not now:** automatic anchors, chosen from mask stability and point visibility; tracks of clips longer than the point tracker's window in one pass (longer clips are tracked in overlapping windows).

### D89. A corner pin lays its layer's plane through a homography

- **Decision:** the `corner-pin` effect lays what its layer drew onto four corners through the projective map (homography) that takes the layer's box to them, and it is drawn as a projected layer is ([D58](#d58-25d-layers-are-planes-projected-through-a-camera)), not by moving pixels.
  - **Corners.** Each of `top-left`, `top-right`, `bottom-right` and `bottom-left` is how far that corner moves from where it rests, in px of the layer, as `bezier-warp`'s are. It rests on the corner of the layer's effect box (what its content fills, or for shapes and text a box the size of its composition centred on its origin), or with `pivots` on where a layer of the same list lands: `"pivots":"screen"` names the four corner pivots `screen-tl`, `-tr`, `-br` and `-bl` that `nakkash ml track … --corners` writes ([D88](#d88-a-track-is-baked-once-and-applied-as-keys-on-pivots)), `["a","b","c","d"]` any four layers, top left first and clockwise. A layer's landing is its anchor on the composition's canvas, through its parents (and its camera, when 3D), taken into the pinned layer's own coordinates through the inverse of its transform (or plane). So the pin meets the pivots wherever the pinned layer is.
  - **Evaluated once.** Evaluation resolves every pin's rest box and corners at each frame into the node's state, and the render, `check` and motion blur all read that one result.
  - **Drawn as a plane.** The layer's content, masks and the effects before the pin are drawn flat on a buffer of their own at twice the most the map magnifies them (up to 4× and D58's largest side), then laid onto the canvas through the layer's transform and the pin with a box filter: each px is the mean of the parallelogram its footprint covers on the plane, from a grid of bilinear probes (at most 16 a side) on the halved copy whose texel is no wider than half the footprint's short side. The copy is picked from the exponent bits of the footprint's sides, and each choice, of copy and of probe count, turns 1/64 past a whole power or a whole px: footprints of exactly a px or two are the common case (a pin that keeps its size, a shutter's small moves), and worked out in `f32` they fall either side of the whole number, so without that slack the GPU took a probe more than the CPU and changed an edge by two levels. D58's trilinear read blurs a shrunk footprint by a whole level and its probes run only along the long axis; text, whose strokes are a px or two wide, loses its edges to it. Text and vector content are drawn at twice the size they land, so pinned UI stays sharp without aliasing; edges fade through the buffer's clear border. Effects after the pin run on the laid result in the layer's coordinates, with the box grown to the corners.
  - **One sampler, both paths.** Laying goes through `Accelerator::project`, so `--gpu` lays a pin with the projected-layer kernel, which takes the box filter as a flag of the landing, under D74's tolerance for projected layers; a pin on an adjusting layer warps what is below on the grid through the same chain, on the CPU or that kernel. No kernel is added.
  - **Motion blur.** With `mblur`, the pin is resolved at each moment of the shutter, pivots and all. When nothing follows the pin, each moment's map is the layer's move then times its pin then, laid once each from one drawing; when effects follow it, the result is laid through each moment's map relative to the frame's.
  - **Degenerate corners.** A quad that is crossed (two sides cross), bent (a corner inside the other three, which folds part of the plane through the horizon), flat (three corners on a line or two at one point) or unplaced (the pinned layer has no inverse) draws nothing of the layer at that frame, and `check` reports it as the error `pin`, frame by frame, naming the corners.
- **Why:**
  - Moving pixels, as the other distortions do, resamples a drawing made at the layer's own size with a bilinear read. Pinned onto a phone screen, a UI shrinks several times and aliases, and parts of it off the canvas are lost before they are pinned; the projected path draws only what the canvas sees, at the size it lands (evidence below).
  - The 2.5D path already holds the plane drawing, the copies, the GPU kernel and their parity; the box filter is a second read of the same copies in the same kernel, not a second path that would drift from it.
  - Offsets from a rest, as `bezier-warp`'s, keep the effect still when written without values, take an import's points as they are, and let a tracked pin be nudged (a bezel inset) without touching its keys.
  - Resolving pins in evaluation gives `check` and the render the same corners, so a quad `check` passes is the quad drawn.
- **Not now:** pins whose pivots sit in another composition; a pin read by another effect's `layer` key with its pin drawn (an effect input is read with its effects, as now); a per-pin quality setting; motion blur on a pinned 3D layer (it draws the frame's pin, sharp); the box filter for projected layers (it costs up to 256 reads a px where they shrink).
- **Evidence:** a 360×720 UI with 14 px text pinned about four times smaller into perspective, and a 110×100 UI with 7 px text enlarged three times, each against a reference supersampled 8× (mean absolute error over the quad; sharpness is the gradient energy over the reference's):

  | Pinned text | Moved pixels (bilinear) | D58's trilinear read | Box filter, 2× plane |
  |---|---|---|---|
  | Shrunk ~4× | 0.1023 error, 1.51 sharpness (aliased) | 0.0663, 0.68 | 0.0166, 0.92 |
  | Enlarged 3× | 0.0884, 0.45 (blurred) | 0.0159, 0.90 | 0.0025, 1.02 |

  On the GPU, the box filter lays planes within 2.1e-5 of the CPU's (D74), a hair off a whole px included, and whole pinned frames within one level; the example `screen-pin` differed by two levels at the phone's rim before the slack. The raster tests hold both numbers (`pinned_text_shrunk_into_perspective_stays_sharp_without_aliasing`, `pinned_text_enlarged_into_perspective_stays_sharp`), the golden `corner_pin` a tracked and a hand-pinned comp with motion blur.

### D90. A frame is drawn from a plan that knows what each drawing is, under one memory budget

- **Decision:** a frame is drawn through a *render plan*, made from its display list before any pixel is drawn. The plan knows, for every node, what it draws (its identity), how many times the frame draws it, how large it is and how long its buffer lives. Drawing then never repeats what the plan knows was drawn, and every buffer it holds counts against one byte budget. The CPU path stays the reference ([D3](#d3-cpu-first-compositor), [D12](#d12-what-repeatable-means)).
  - **Quality is not traded.** These rules bind every step of this decision and every pull request that implements it:
    1. The precise path stays bit-identical. Every optimisation gives the same bytes as the reference on every example, golden image and corpus frame; a pull request that changes one byte does not merge.
    2. Memory limits change only speed. Under the budget or under memory pressure the render evicts, draws again, draws in tiles or goes sequential. It never lowers resolution, precision or quality, and a test draws heavy fixtures under a tiny budget and compares them with an unbounded run, bit for bit.
    3. Storage precision does not change. Grids stay `f32` in the precise path; `f16` or fixed-point storage enters only where it is proven bit-identical.
    4. Approximate kernels (a pyramid or recursive blur, noise octaves drawn small) are not part of this work. If they come later, they are opt-in, for previews only, never the default of any render, `Quality::Fast` included, and only once the maintainers approve them.
    5. Cache keys are complete: a hit never returns a different drawing. A verify mode draws a sample of hits again and compares the bytes; the corpus runs it on the render machine.
    6. Exact means the same operations on every pixel, in the same order. Work may be scheduled, skipped, reused or vectorised; the arithmetic may not change. Not allowed: fused multiply-add where the reference multiplies and adds; a reciprocal multiply for a divide; coordinates stepped incrementally where the reference computes them; sums reordered or compensated; another blur algorithm; a clamp rewritten as `max` then `min`; bilinear sampling rewritten as separable passes; glyph coverage quantised into an atlas; another rasteriser.
    7. Every faster kernel keeps the one it replaces as its reference, and tests compare the two bit for bit: on edge inputs (NaN, infinities, values at half a level), at every SIMD level forced in turn, with pooled grids and halos filled with junk, and at several thread counts. A render machine run compares `--digest` hashes of the corpus before and after.
  - **Kernel references.** The checks of rule 7 are a test module of `nakkash-raster` that every faster kernel's tests use: edge inputs (signed zeros, subnormals, NaN payloads, infinities, both neighbours of every half level, sums an `f64` holds only rounded), run lengths that reach every SIMD tail, output grids guarded and filled with junk, a comparison by bits that names the first difference, every instruction set the processor has forced in turn, thread pools of any size and work split into tasks of any size. Its own tests show it finds a change of one unit in the last place, of a NaN's payload or of a zero's sign, and a write past either end. Today every effect gives the same bits split into tasks of 1, 7, 64 and 333 items as at the usual split, and a frame of every family of work the same bits on 1 to 8 threads at every level with pooled grids clobbered. Clippy refuses `mul_add` in the workspace, and a frame is refused on a thread that flushes subnormal floats to zero, as a host built with fast math may set it to. The rasteriser's instruction sets do not give the bits of its portable fallback (8 bytes of the test frame's 76,800 differ between NEON and the fallback, as [D12](#d12-what-repeatable-means) allows), so a kernel is compared with its reference at the same level, never across levels.
  - **The rasteriser crate is owned.** `vendor/vello_cpu` is the published crate (0.4.0), verbatim but for changes marked "Changed for nakkash (D90)", patched in for crates.io and outside the workspace's lints. Its highp `pack`, a scalar loop at every level as published, converts 16 values at a time: the published multiply and add, then a truncating, saturating conversion to integers held to 0 to 255. That is the published `as u8` for every `f32`, which a test checks over all 2³² of them at every level, beside a test of the packed rows against the published loop, kept as the reference. It is 3.3 times as fast with AVX2, 2.3 with SSE 4.2 and 1.1 with Neon; without vectors the published loop stays. A change upstream would let the copy go.
  - **Identity.** Every node of the display list has a 128-bit identity, computed bottom-up. It covers every field of the node, floats by their bits; pictures and video frames by a hash of their pixels, made once when they are decoded; fonts by their data's id, which no other font data ever has; and its children's identities. A drawing's key adds what it is drawn at: the transform's bits, the grid's size and the render's options. The hash is written by destructuring each type, so a field added to the display list does not compile until the identity covers it. Keys that may one day outlive a run also cover the engine's version and the list's schema.
  - **One drawing per identity in a frame.** The plan counts the uses of each identity in the frame: a layer an effect reads, children several layers share, a matte's source, a layer drawn alone. What is used more than once is drawn once and kept until its last use; what is used once is drawn as it is used. This replaces `Reads`' node addresses and use counts.
  - **One frame per identity.** A frame whose list has the identity of a frame drawn before in the same render is copied, not drawn: to the encoder, to PNG files and to `--digest`.
  - **Bounds and regions.** The plan computes each node's drawn box forward and the region its readers read backward, through each effect's reach, so a layer drawn alone gets a grid of its drawn box grown by that reach instead of the canvas. An effect that reads its whole box keeps the box ([D76](#d76-a-composition-is-drawn-at-the-size-it-lands-at)): it is a barrier the region cannot cross. Regions come one effect family at a time, each proven bit-identical over the corpus before it is on.
  - **Pooled grids.** Freed grids go to a pool, by size, inside the budget. A grid taken from the pool is either written whole or cleared explicitly, and the halo a kernel reads past its region is padded explicitly: nothing relies on memory the kernel zeroed. In tests a clobber mode fills every grid taken from the pool with a signalling pattern, so a read of stale memory changes the bits and fails.
  - **A cache that knows the plan.** Kept across frames, a drawing is admitted only when drawing it again costs more than keeping it and a later frame is known to use it; it is evicted by next use, pinned while read, and threads drawing the same identity wait for one drawing instead of making two. `render`, gates, `vary` and `batch` share it.
  - **Time intervals.** Later, the plan knows over which frames each node is constant, from its keys and the expressions that make it: the cache's next use, held frames and gate results become exact rather than guessed, and bakes stop at constants.
- **Resource safety.** Neither the workstation nor the render machine is worn out by a render: memory stays bounded, the machine does not swap, the disk is not written but for what was asked, and the processors stay within their limits.
  - **One byte budget.** Every buffer the render holds counts against one budget: drawings kept for later uses, pooled grids, and the float grids being worked. It is `--mem-mib N` and by default a quarter of the machine's memory, at most 4 GiB; the machine's memory is the smaller of its physical memory and the memory limit of the container (cgroup) the render runs in.
    - A quarter leaves the rest to the operating system, the programs open beside it and the encoder. 4 GiB holds the peak heap of every corpus sample's settled frame (median 141 MiB, largest 1.9 GiB) with room to keep drawings, and seven 2-core jobs at once on the 62 GiB render machine stay under half of it. On the 36 GiB workstation it is 4 GiB; on an 8 GiB machine, 2 GiB.
  - **Over the budget the render gets slower, never larger.** In this order:
    1. the pool lets its free grids go;
    2. kept drawings are let go, the least soon needed first, and are drawn again if read again. They take at most half the budget: letting one go draws it again at the cost of more working grids than it holds, so working grids past the budget do not by themselves let kept drawings go; memory pressure, which lowers the budget, does;
    3. frames stop being drawn side by side: a thread waits before it starts a frame while the frames being drawn hold the budget, so one frame is drawn at a time, on every thread ([D75](#d75-a-frame-is-drawn-on-every-thread-of-its-render));
    4. once regions land, layers drawn alone are drawn in regions or tiles.
    A single frame whose own working grids exceed the budget is still drawn, since refusing it would give no frame; it is reported, and `--stats` says by how much. No step changes a pixel: each draws what it would have drawn, later.
  - **No swap.** The render reacts to memory pressure before the operating system swaps. On macOS it reads the kernel's memory-pressure level; on Linux, the container's `memory.current` against `memory.max`, its pressure stall information (`memory.pressure`, or the machine's), and `MemAvailable`. At a warning the budget falls to three quarters of what is held and the pool empties; at a critical level the budget falls to the working grids alone, so nothing is kept and frames go one at a time. The budget comes back once the pressure has been normal for a few seconds. The levels are read at frame starts and at large grids, at most ten times a second.
  - **Storage and wear.** A render writes only what it is asked to write.
    - There is no disk cache and no intermediate file on disk; everything kept is in memory, inside the budget.
    - A video is encoded as frames are drawn, never through a sequence of PNG files; `--digest` writes nothing at all and prints each frame's hash, which is how renders are compared in tests and on the render machine.
    - A disk cache, if one comes, is opt-in, capped in size, written once per key and named by its key (content-addressed, so a file is never rewritten), and on by default only on the render machine.
    - `--stats` reports the bytes the process wrote to storage, as the operating system counts them.
  - **Processors and heat.** `--threads` caps every thread a render uses (D75). Without it, the render uses the performance cores on macOS, so the efficiency cores stay free for the desktop, and on Linux the processors its container or job may use: on the render machine, the cores `box-run` gave the job. Frames running side by side multiply memory, not threads, and the budget limits them as above.
  - **Telemetry.** `--stats` prints a summary at the end of any command that draws: wall, user and system time; the peak resident memory; the budget, the most it held and how often it was reached; drawings evicted, grids pooled and reused; frames held back; pressure events; major page faults and swap traffic where the system reports it; and the bytes written.
  - **Gates.** Tests run the real binary on heavy fixtures and assert that its peak resident memory stays within the budget and its overhead, that a render to `--digest` writes no byte to storage and leaves no file, and that frames drawn under a budget of 1 MiB are the frames drawn without one.
- **Why:**
  - Measured on the 4K frame of the heaviest template in the corpus set: it draws 512 layers alone, of which 57 are distinct, so 61.6 % of its time draws again what it drew. Keyed by content, the same frame takes 2.13× less time on two cores of the render machine (1,012 s to 475 s), 1.86× less on the workstation and 2.2× less with `--gpu`, with GPU uploads falling from 23.7 to 6.9 GiB, and the same bytes. With held frames and a pool besides, 2.43× (417 s).
  - Identical consecutive frames are 26 to 98 % of the templates that have any (m629: 313 of 449; m749: 59 of 60); skipping them took m629 from 37.4 to 11.5 s.
  - Every layer with effects gets a canvas-sized float grid, 188 MiB at 4K; 78 % of the effect time of that frame falls outside what was drawn, and on Linux the frame allocates 456 GiB, with 37 % of its processor time in the kernel zeroing new pages. Reusing grids alone cut m647's system time from 68 to 7 s.
  - The prototype's cache had a budget but nothing else did: with 4 GiB of cache, m647's resident memory went from 170 MB to 5.9 GB. A budget that covers only the cache does not bound a render.
  - Templates that repeat nothing were slower with the cache (m639 67.0 to 69.6 s, m749 397 to 409 s), which is why admission needs a known later use.
  - Hashing through `Debug` cost 342 s of a 3,009 s gate run, and keying pictures by address returned the wrong drawing for 7 of 30 frames: identity is a real hash over content.
- **Rejected:**
  - Keys by address: they cannot see equal nodes, and a freed address comes back with other content.
  - 64-bit identities: a collision draws the wrong pixels without a sign.
  - A second renderer for imported scenes: it would drift from the reference the corpus measures.
  - Cropping effects after drawing on a canvas-sized grid: it cut effect pixels to 2 to 5 % and the time by at most 5 %; the cost is in drawing, allocating and laying canvas-sized grids.
  - Letting memory grow past the budget to stay fast, or swapping to stay alive: the machines' wear is a cost.
  - Intermediate files on disk, and PNG sequences as a step towards a video.
- **Packing, measured and read.** On the render machine the precise path's float-to-byte packing (`pack` of `vello_cpu`'s f32 pipeline) takes 10 to 22 % of light frames. Its dispatch is right: the profile shows it under the AVX2 level, as the rest of the rasteriser. The loop itself is scalar at every level, in the released 0.4.0 and upstream alike, marked as not yet vectorised. So no build target or level choice speeds it up; an exact SIMD form does, multiplying and adding separately as the scalar code does, truncating and saturating as `as u8` does (NaN to 0), and transposing the tile's columns in registers. It belongs upstream, or in a copy we own until upstream takes it, and it lands only with tests that compare it with the scalar loop bit for bit at every level.
- **Not now:** occlusion culling; tile damage between frames; fused pointwise passes; a GPU path that keeps drawings resident by identity; a disk cache across runs (keys are made persistence-safe now, so it stays cheap to add).
- **Order of work.** Each step merges only when every example, golden image and corpus frame is bit-identical to before, tests for equal nodes and reused addresses pass, resident memory stays within the budget, and the workspace's checks pass.

  | Step | What | Measured or expected |
  |---|---|---|
  | 0 | Side fixes: no copy of a displacement map, of glow's layer or of a distortion's source; an exact SIMD form of the rasteriser's float-to-byte packing | copies are 13 % of the 4K frame on the workstation; packing 10 to 22 % of light frames on the render machine |
  | Guard | The budget and its accounting, the pool, memory-pressure hooks, `--mem-mib`, `--stats`, the memory and no-write gates | sys time down 45 to 90 % on effect-heavy templates; resident memory bounded |
  | 1 | Identity, and `Reads` by identity with matte sources shared | 1.9 to 2.4× on nested templates |
  | 2 | Frame identity in render, encode and `--digest` | 1/(1 − held share): 2.8× on m629 |
  | 3 | Bounds and regions, one effect family at a time | memory and uploads divided by canvas over region |
  | 4 | The cache across frames, with admission, next use, pinning and in-flight sharing, and its verify mode | 2.1 to 3.4× with the above on nested templates, within the budget |
  | 5 | Gates on the plan: `contrast` once per sample, results of identical frames kept | at least 1.8× on the heaviest gate run |
  | 6 | Time intervals and bakes that stop at constants | import time divided by layout passes and threads |
