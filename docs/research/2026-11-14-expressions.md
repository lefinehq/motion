Status: Draft · 2026-11-14

# Expressions: what the corpus needs, and how to bake it

Background for [D38](../design/decisions.md#d38-expressions-are-baked-at-import): an expression is evaluated at import, at every frame, and written into the scene as keys. This note asks four questions:
- What do the expressions in our local corpus actually do?
- Which embedded engine should evaluate them?
- Which parts of the expression language can be reproduced exactly?
- How should sampled values become editable keys?

A fuller report, with every source linked, stays local with the samples, because it names products (D18).

## What the local corpus holds

All 236 local projects, read with `motion inspect --json` (2026-11-14).

- **How many:** 172 of 236 projects (73%) carry expressions, 7,697 in all, 1,260 distinct. Per project: median 13, 90th percentile 122, at most 814.
- **Which engine:** each project names the engine it evaluates expressions with, in the `ExEn` chunk.

  | Engine | Projects |
  |---|---|
  | `javascript-1.0` (the modern engine) | 109 |
  | `extendscript` (the legacy engine) | 94 |
  | no `ExEn` chunk (older files, legacy engine) | 33 |

  So more than half the corpus runs under legacy semantics.
- **Syntax:** standard JavaScript parses 1,247 of the 1,260 distinct sources. The other 13 share one legacy form, `if (x == 1) 100 else 0`, a statement with no semicolon before `else`.

### What they do

| Kind | Expressions | Projects |
|---|---|---|
| Read a control or another property (`comp(…).layer(…).effect(…)(…)`, `transform.scale`) | 5,570 | 141 |
| Other: constants, comp size, generated code | 702 | 37 |
| `wiggle` | 416 | 20 |
| A function of `time` | 269 | 36 |
| Property-tree introspection (`propertyGroup`, `numProperties`; one test project) | 151 | 1 |
| Easing scripts over the keys (`nearestKey`, `key(n)`, `Math.pow`) | 140 | 28 |
| `sourceRectAtTime`: layout of another layer | 130 | 15 |
| `loopOut` / `loopIn` | 130 | 27 |
| Bounce and inertia scripts (`Math.sin`, `Math.exp` after the last key) | 125 | 27 |
| `random` / `seedRandom` | 21 | 5 |
| `valueAtTime` | 10 | 4 |
| Text selector expressions (`textIndex`) | 8 | 1 |

Expressions sit mostly on transforms (1,919) and fills (1,851 layer fills, 433 shape fills), then on effects.

### What baking each project needs

Each project is counted at the hardest thing any of its expressions needs.

| Needs | Projects | Share of projects with expressions |
|---|---|---|
| Only the project's own values, keys and time | 131 | 76% |
| Also our layout (`sourceRectAtTime`, source text) | 15 | 9% |
| The original noise and random generators (`wiggle`, `random`) | 21 | 12% |
| A third-party plugin's runtime (generated, encoded sources) | 5 | 3% |

- **Deterministic:** 85% of the projects with expressions need nothing but data we already decode, plus our own text layout.
- **Arrays:** 76 legacy-engine expressions in 4 projects appear (statically) to use arithmetic on arrays. The legacy engine overloads `+ - * /` on arrays; the modern one does not. Baking must give each project its own engine's semantics.
- **Load:** the busiest project (814 expressions) at 30 to 60 frames a second over its span is up to about a million evaluations, so the engine's per-call cost matters.

## The language, as baking sees it

- **Two engines, one evaluator.** The modern engine follows ECMAScript 2018 and the legacy one ECMAScript 3. Both overload `+ - * /` on arrays (`position + [100, 0]`), so our evaluator must do the same for every project.
  - The application's reference lists the legacy engine's quirks:
    - `if` and `else` on one line without braces;
    - snake_case aliases (`this_comp`, `in_point`);
    - `this(…)` for `thisLayer(…)`.
  - The engine a project names decides which quirks to accept.
- **Arrays.** Arithmetic mixing an array and a number touches only the first component; two arrays combine component by component, and the shorter is padded. These are the open-source Lottie player's rules, which match the reference's description.
- **What must be provided:**
  - globals: `time`, `value`, `thisComp`, `thisLayer`, `thisProperty`, `comp(name)`, `posterizeTime`;
  - properties: `valueAtTime`, `velocityAtTime`, `key(n)`, `numKeys`, `nearestKey`, `loopIn`/`loopOut` (cycle, pingpong, offset, continue), `wiggle`, `smooth`, `propertyGroup`;
  - interpolation: `linear`, `ease`, `easeIn`, `easeOut`;
  - vector maths: `add`, `sub`, `mul`, `div`, `clamp`, `length`, `normalize`, `dot`, `cross`, `lookAt`;
  - colour: `rgbToHsl`, `hslToRgb`;
  - time: `framesToTime`, `timeToFrames`;
  - random: `random`, `seedRandom`, `gaussRandom`, `noise`;
  - layers: `index`, `inPoint`, `outPoint`, `parent`, `effect(name)(param)`, `sourceRectAtTime`, `toComp`/`fromComp`, `marker`;
  - comps: `layer(name | index)`, `width`, `height`, `duration`, `frameDuration`, `displayStartTime`.
- **Not exact:**
  - **`ease`:** the shape of its curve is not documented.
  - **`wiggle`, `random` and `noise`:** they use a generator nobody has published or reverse-engineered. Their seeds depend on internal layer ids. Open-source players each use their own noise, and one fixes `wiggle`'s frequency at 5.
  - **What we do:** bake these with our own documented, seeded noise, and mark the keys *approximate* in the import report. The ledger shows how far they land.
- **Not at import:** reading pixels (`sampleImage`), external data files that were not imported, `Date`, and values a template expects its user to change later. These are reported, as D38 says.

## Prior art

- **Exporters that bake run inside the original application.** The Lottie exporter asks the application for each property's value at every frame. It writes linear keys, drops a key between two collinear ones, and rounds to three decimals. It does no curve fitting. Before export it also rewrites `+ - * /` into helper calls with a JavaScript parser. That is the operator rewrite we need, and its code is MIT.
- **Players that evaluate at playback:** the Lottie web player (MIT, `eval` with shadowed globals) and ThorVG (MIT, an embedded C engine). Skia's player only offers a hook for the host to evaluate expressions. rlottie and Qt's player do not support expressions.
- **No project outside the original application bakes these expressions into keys.** We would be the first.

## Engines

| Engine | Licence | Build | Speed (V8 suite, higher is faster) | Notes |
|---|---|---|---|---|
| [Boa](https://github.com/boa-dev/boa) 0.22 | MIT or Unlicense | Pure Rust, Rust 1.91 (ours) | about 265 | 95.6% of test262. Has global and host-function APIs, and loop, recursion and stack limits. About 12 MB in a release binary. |
| [rquickjs](https://github.com/DelSkayn/rquickjs) 0.14 (QuickJS-NG) | MIT | Needs a C compiler | about 940–1,000 | About 1.5 MB. Built-ins can be chosen one by one, so `Date` can be left out. Has an interrupt handler. |
| V8 (`rusty_v8`) | MIT | 39 MB prebuilt download, or a source build of 30+ minutes | fastest | Too heavy for an import step. |
| Nova, Starlight | MPL-2.0 | | | Not allowed (D13). |

Speeds are from the [js-engine-benchmark](https://github.com/ahaoboy/js-engine-benchmark) of 2026-11-14.

**Determinism, whichever engine we pick:**
- `Math.random` is seeded from the clock or unseeded, and `Math.sin`, `exp` and `pow` call the platform's maths library. Rust's own documentation calls the precision of `f64::sin` non-deterministic across platforms.
- So `Date` goes, `Math.random` is replaced, and `Math.*` runs through the pure-Rust [`libm`](https://crates.io/crates/libm). Then a bake gives the same keys on macOS and Linux.

## From samples to keys

- **Fit each component in 1D.** Time is the parameter, so the 2D reparameterisation of [Schneider's algorithm](https://github.com/erich666/GraphicsGems) is not needed.
- **Each segment is a cubic Hermite in time.** With handles reaching a third of the way (`speed(out, .333, in, .333)` in our format), time along the curve is linear in its parameter. So with both end values pinned, the best two slopes are a closed-form 2×2 least-squares solve: no iteration, deterministic.
- **Choose keys by splitting, then thinning.** Split at the worst frame until every frame is within tolerance. Then remove interior keys in order of least added error while the tolerance holds, as [curve_fit_nd](https://github.com/ideasman42/curve-fit-nd) (BSD-3) does.
- **Jumps and units.**
  - Steps (`posterizeTime`, per-frame random) become `hold` keys, and no curve is fitted across a jump.
  - Tolerances are in each channel's unit: pixels, degrees, or a fraction of opacity.
  - Sample between frames where a layer is re-timed.
- **Fallback:** per-frame linear keys, as the exporter writes them, wherever fitting would not save keys.

## Recommendation

1. **Engine: Boa, behind our own trait** ([D37](../design/decisions.md#d37-engines-stay-behind-our-own-types)). It is pure Rust, matches our toolchain, has a sandboxing API, and conforms well enough.
   - It depends only from the import, and a boundary test fails if anything that renders links it, as for the network in D39.
   - The first spike measures evaluations per second on a bounce script whose `valueAtTime` calls back into Rust. If Boa misses the budget (the busiest project's million evaluations within seconds), rquickjs replaces it behind the same trait.
2. **Determinism layer:** no `Date`, our own `random`, `libm` maths, and a cap on instructions and recursion per evaluation.
3. **Rewrite the source before evaluating it:**
   - arithmetic operators become helper calls with the array rules above;
   - the last statement's value becomes the result;
   - for legacy-engine projects only, also accept `if … else` on one line, the snake_case aliases and `this(…)`.
4. **Host objects over the decoded project:**
   - comps, layers and the property tree, with their keys;
   - effect parameters by name or index;
   - other expressions, evaluated on demand and memoised per frame, with cycles reported;
   - our own text layout for `sourceRectAtTime`.
5. **Bake and report:**
   - sample at every frame, fit with the 1D Hermite fitter, `hold` at jumps;
   - for each property, report the source, the engine, whether the result is exact or approximate, the number of keys, and the largest error per channel.
6. **Measure:**
   - On the ledger: s740 (easing scripts over its first two seconds) and s737 (title colour from a colour control, timing).
   - On the whole corpus: how many expressions bake, how many are approximate, how many are reported.

**Expected reach:** the 85% of projects with expressions whose expressions read only project data and our layout bake exactly. 12% bake with approximate noise, and 3% (a plugin's generated code) are reported.
