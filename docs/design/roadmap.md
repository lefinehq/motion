Status: Draft · 2026-11-13

# Roadmap

v1 is a variant engine: designed projects come in, and professional variants go out. We start with a short proof phase on real projects, then build in four milestones. Each milestone ends with a public release. Creating videos from scratch gets its own roadmap once variants work.

## Proof phase (about two weeks, before M0)

These are throwaway tests on 3–5 of the lead designer's real projects. Each test has a pass line agreed before it runs. If a test fails, we change the design before building on it.

| Test | Pass line |
|---|---|
| Export 3 projects through a minimal exporter | ≥ 80% of layers and properties map exactly; the gaps are listed |
| Render the 3 hardest frames with Vello and a small compositor | The designer judges them on par with the tool's own render |
| Render a full master and encode it | Faster than real time, including encoding, on this machine |
| Make one variant by hand-written edits: German, 9:16, 15 s | Done in under 30 edit lines, and `check` catches every problem we planted |
| Shape Latin, Arabic, CJK and emoji text | Glyph positions within 0.5 px of HarfBuzz |

## M0: Foundations

- The public repository, with its licence, contributing guide, code of conduct, security policy and templates.
- Continuous integration: formatting, lints, tests and golden-image comparisons.
- The benchmark harness.

## M1: Scenes without pixels

**Builds:**
- reading, compiling and evaluating scenes
- `check`, edits and `explain`

**Done when:**
- the token targets are met (see [Overview](overview.md#goals));
- `check` catches every planted defect.

**Release:** 0.1.

## M2: Import and first frames

**Builds:**
- the exporter
- the coverage report
- drawing, compositing and encoding for every core-tier feature the reference projects use

**Done when:**
- imported masters match the tool's renders within the fidelity limits (see [Quality](quality.md#fidelity));
- the text-shaping tests pass;
- the designer signs off.

**Release:** 0.2.

## M3: Variants

**Builds:**
- the variant surface: roles, beats, style profile, locks
- swap, reframe, cut down, localise and reinterpret
- batch rendering

See [Variants](variants.md).

**Release:** 0.3.

## M4: Proof on a real campaign

Several masters, each turned into a full set of variants. We measure:
- the designer's approval rate;
- blind votes comparing AI variants with hand-made ones;
- tokens and time per variant.

**Release:** 1.0.

## Next

- A roadmap for creating videos from scratch.
- The extended tier (see [Features](features.md#extended-tier-phase-2)).
- A direct `.aep` reader.

## Benchmarks

Quality work at each milestone is described in [Quality](quality.md#when). The `bench/` folder holds:

- **Reference projects:** the designer's masters, with the tool's own renders as ground truth.
- **Coverage per project:** the share of layers and properties that import exactly.
- **Fidelity per frame** against the tool's render.
- **Variant costs:** tokens, edits and wall-clock time per variant, and render time.
- **Approval rate:** the share of variants approved without changes.
- **Text-shaping tests:** Latin, CJK, Arabic, Hebrew, emoji.
- **A planted-defect set** that `check` must catch.
- **Competitor baselines:** the reference scene in HyperFrames and Remotion, for tokens and render speed.

## Risks

| Risk | What we do about it |
|---|---|
| The reference projects use features beyond the core tier | The coverage report shows the gaps early; the most common gaps move into v1 |
| Vello's API changes quickly | Pin versions; keep Vello behind a small interface |
| Text looks worse than the tool's | Text-shaping tests from M2; compare against the tool's renders |
| Expressions can't be mapped | Bake them to keyframes; report them in the coverage report |
| The tool's scripting API changes between versions | Test the exporter against each major version |
| Reinterpretation drifts off-style | Limit it to the style profile; the designer's approval rate gates releases |
| The name collides with an existing product | See [Q1](decisions.md#q1-name-and-packages-deferred) |
