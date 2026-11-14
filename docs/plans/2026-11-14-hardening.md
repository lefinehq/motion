# Hardening while we build

**Goal:** take the risks that would cost the most later off the table now, while motion is early and has no outside users, without slowing the fidelity work. Every item here either catches a regression the moment it happens or makes the next feature cheaper. Nothing is done for its own sake.

**Done means:**
- A scene written today keeps reading and drawing the same way, or a deliberate format change shows up in review with its migration.
- Every fidelity pull request shows its before and after numbers over all local samples, produced by one command.
- The whole chain, from `.aep` bytes to pixels, is guarded in CI by fixtures the repository may hold.
- What we know about the `.aep` format only by inference is listed, and each inference is checked against every local sample.
- Engines we depend on stay behind our own types, so replacing one touches one crate.
- Expressions have a design decision before they have code.

## The risks, in order of what they would cost later

| Risk | Cost if left | Answer | When |
|---|---|---|---|
| The scene format changes under scenes already written | Every stored scene, template and agent prompt breaks; grows with every user | Version policy and a compatibility corpus (H1) | Now |
| Fidelity regresses or improves without anyone seeing it | Fixes are guessed, regressions ship; grows with every feature | `motion fidelity` and a committed ledger of numbers (H2) | Now |
| The import chain breaks on files only a laptop has | CI is green while imports are broken | Import goldens on the repository's fixtures (H3) | Now |
| Format knowledge we inferred is wrong | Silent errors on projects unlike our samples | Evidence table and invariant sweeps (H4) | With each decoder change, starting with text in 8c |
| A young dependency changes its API | Changes spread through every crate | Dependency boundary rule (H5) | Rule now, `FontData` in 8c |
| Expressions grow into a second runtime | Non-deterministic renders, a JavaScript engine in every render | Bake at import, decided first (H6) | Before the expressions step |
| No public specification | Others cannot implement or check the format | Specification and conformance suite | Before 1.0, not now |

## H1. Format stability

Decision D36:
- `"motion": 1` stays while changes are additive: a new key, a new value form, a looser limit. Old scenes keep their meaning.
- A change that alters what an existing scene means, or rejects one that was valid, bumps the version, and `motion-format` gains a migration from the previous version. Every older version stays readable through the chain of migrations.
- Until 1.0, breaking changes are allowed, but never silently: they must change the compatibility corpus in the same pull request.

The compatibility corpus is `crates/motion-format/tests/compat/`: scenes that cover every key and value form the format accepts today, each with a snapshot of how it reads (the parsed document) and a hash of three of its frames. A test reads every scene in it and compares both. If a change alters a snapshot, the difference shows up in review. A new key adds a scene to the corpus.

## H2. Fidelity ledger

`motion fidelity <manifest.toml>` runs, for each sample in a local manifest, the import, the text geometry check and `motion diff`. It then writes one line of numbers per sample:
- what the import could not support, by kind;
- the worst text line and how many texts are within tolerance;
- mean error and failing fraction, both where drawn and over all drawn;
- the same, for settled frames only.

The command compares the result with the last ledger and prints what improved and what regressed.

The manifest names local files: projects, reference frames, fonts and layers to ignore. It stays out of the repository. The ledger holds only sample ids and numbers, never names (D18), and lives in `docs/research/fidelity/ledger.json`. A pull request that changes fidelity commits the new ledger, so the change shows up as numbers in review.

## H3. Import goldens in CI

Each fixture in `crates/motion-import/tests/fixtures` (the ones the repository may hold) is imported and drawn at its first, middle and last frame. The frames are compared with goldens in the same way as the renderer's golden tests. This is the only check that runs the whole chain in CI.

## H4. Evidence for inferred format knowledge

`docs/design/aep-evidence.md` lists every field the decoder reads by inference, not from documentation, with what we believe it means. Each belief has a check in the local sweep that holds it over every sample, and the table records how many samples hold it. Two examples:
- a composed line's origin y equals its baseline;
- a glyph run's advance box is as wide as the segment.

When a check fails on a new sample, the belief is wrong. We start with the composed-line cache, which the text check now relies on.

## H5. Dependency boundaries

Decision D37:
- Public APIs may use vocabulary crates that are built to be shared: `kurbo` and `peniko`.
- Engines stay behind our own types: `parley`, `vello_cpu`, `binrw`, `winnow`, and later the JavaScript engine.

`GlyphRun.font` was the one case: the shared font bytes type, re-exported through the shaper. It is now named from its vocabulary crate, and a test fails when a public line names an engine.

## H6. Expressions are baked at import

Decision D38, written before the expressions step:
- An expression is evaluated at import with an embedded JavaScript engine at every frame of its layer's span. It is written into the scene as keys, fitted to curves within a tolerance.
- The report keeps the expression's source and says what it became.
- A render never runs JavaScript. It stays deterministic, fast and safe on untrusted scenes.
- An expression that reads something the import cannot know, such as an input the user sets later, is reported, not guessed.

A live expression in the format would be a separate decision, and only if a real template needs it.

## Order

1. H1, with D36: a small pull request.
2. H2: the command and the first ledger, made with what is merged today.
3. H3.
4. Step 8c, faux bold, measured by the ledger. H4 for the text cache and H5 for `FontData` go in the same pull request.
5. D38, then the expressions step.
