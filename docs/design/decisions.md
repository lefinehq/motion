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

- **Decision:** a layer draws a video clip with `"video":"<file>"`, an `.mov` or `.mp4` file relative to the scene file, stretched over the layer's required `box`, as an image is ([D41](#d41-image-layers-name-their-file-and-carry-their-box)). At its own time `t` the layer draws the frame showing `t` seconds into the clip, and nothing before the clip starts or from its end on. Clips are read by `motion-video`, with our own reader of the MP4 and QuickTime containers and one `Decoder` trait a codec implements; QuickTime Animation and Photo JPEG are the first. The engine opens each clip once per session and keeps its decoder and last frames, so a render going forward decodes each frame once. H.264 comes later under [D13](#d13-codec-licence-policy): from the operating system, or a decoder loaded at run time, never compiled in. See the [plan](../plans/2026-11-16-video.md).
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
