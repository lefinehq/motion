# Expressions baked at import

**Goal:** an imported project's expressions become ordinary keys in the scene ([D38](../design/decisions.md#d38-expressions-are-baked-at-import)). Each one is evaluated at import, at every frame of its layer, by an embedded JavaScript engine, and the samples are fitted back to a few editable keys. A render never runs JavaScript. The [research](../research/2026-11-14-expressions.md) says 85% of the projects with expressions can be baked exactly from data we already decode.

**Done means:**
- `motion import` bakes every expression it can evaluate.
  - For each one, the report gives what it became: exact or approximate, how many keys, and the largest error per channel.
  - It reports, with the reason, what it could not evaluate.
- s740's first two seconds and s737's title colour match their references, and the ledger shows it.
- A local sweep bakes every expression of the 236 local projects and counts the outcomes.
- Baking is deterministic: the same project gives the same keys on macOS and Linux.
- Nothing that draws links the JavaScript engine; a test fails if something does.

## Measured before planning

A spike on the corpus (2026-11-15):

| Question | Answer |
|---|---|
| Speed, a 1.3 KB easing script reading keys through Rust | Boa 69,000 evaluations a second, once compiled; QuickJS-NG 33,000 when parsing each frame |
| Speed, a bounce script with `velocityAtTime` | Boa 72,000; QuickJS-NG 75,000 |
| Speed, a link to a control | Boa 5.2 million; QuickJS-NG 870,000 |
| Rewriting `+ - * /` into helper calls on Boa's syntax tree, printing it back and parsing it again | 1,247 of 1,260 distinct sources; the other 13 are the legacy `if (x) 100 else 0` |
| Same results from both engines | yes, on every spike expression |

- **The busiest project** (814 expressions) is about a million evaluations, a few seconds on one thread. Properties are independent once their inputs are baked, so they also spread over threads.
- **Boa wins on fit, not only on speed:** pure Rust, our toolchain version, a sandboxing API, and its parser and syntax tree for the rewrite.
- **Sloppy mode.** Expressions assign to undeclared names (`t = time - key(n).time`), so they run in sloppy mode.

## Design

### The crate `motion-expr`

It evaluates expressions and fits samples, and knows nothing about `.aep` files.

- **`Engine`:** our trait over the JavaScript engine (D37). Boa is the one implementation, private to the crate.
  - It compiles a source once into a `Program`, then runs it for each frame with `time` set.
  - Limits per run: loop iterations, recursion and stack.
  - Determinism:
    - no `Date`;
    - `Math.random` is replaced by our seeded generator;
    - every `Math` function that is not exact in IEEE arithmetic runs on `libm`.
- **`rewrite`:**
  - The source is parsed with Boa's parser.
  - Arithmetic operators, compound assignments and unary minus become calls to `$add`, `$sub`, `$mul`, `$div`, `$mod` and `$neg`. These follow the array rules: component by component, the shorter padded; a number with an array touches the first component; strings concatenate.
  - The tree is printed back and compiled.
  - Legacy-engine projects also accept:
    - `if … else` on one line without a semicolon, repaired where the parser reports it and recorded;
    - the snake_case aliases;
    - `this(…)`.
- **The library:** the language's functions that need no host, written once in JavaScript or Rust:
  - interpolation: `linear`, `ease`, `easeIn`, `easeOut`;
  - vector maths: `add`, `sub`, `mul`, `div`, `clamp`, `dot`, `cross`, `length`, `normalize`, `lookAt`;
  - colour: `rgbToHsl`, `hslToRgb`;
  - angles and time: `degreesToRadians`, `radiansToDegrees`, `framesToTime`, `timeToFrames`;
  - random: `random`, `gaussRandom`, `seedRandom`, `noise`, on our generator.
- **`Host`:** the trait the importer implements. It lets an expression read:
  - the comp and layer it runs in;
  - other comps and layers by name or index;
  - properties by match name, display name or index;
  - each property's keys, its value and velocity at any time, and its markers.

  The crate turns these into the expression objects (`thisComp`, `thisLayer`, `thisProperty`, `comp()`, `effect()`, `key(n)`, `nearestKey`, `loopIn`/`loopOut`, `valueAtTime`, `velocityAtTime`, `wiggle`, `smooth`, `posterizeTime`) in a JavaScript prelude over a few native calls.
- **`fit`:** samples to keys, one channel at a time.
  - Each segment is a cubic Hermite in time, with handles reaching a third of the way, so the two slopes are a closed-form least-squares solve.
  - Keys are chosen by splitting at the worst frame until every frame is within tolerance, then by removing keys while the tolerance holds.
  - Jumps become `hold` keys.
  - Where no fit saves keys, the fallback is linear keys at every frame.

### The import

- **The original keys, evaluated as the project means them.** A property's value at a time comes from its own keys: temporal ease, spatial curves and holds. This is what `value` and `valueAtTime` read in an expression.
- **The bake.**
  - Where a property has an enabled expression, it is evaluated at every frame of its layer, in the layer's time. Other expressions it reads are evaluated on demand and remembered per frame; a cycle is reported.
  - The fitted keys replace the property's keys in the scene. The import report records, per property:
    - the source's first line;
    - the engine;
    - exact or approximate;
    - the keys before and after, and the largest error.
- **What is reported, not guessed:** pixels (`sampleImage`), files the project did not import, `Date`, `$`, and code that a plugin generated and encoded.
- **Approximate:** `wiggle`, `random`, `gaussRandom` and `noise` use our own generator, seeded by the layer, the property and `seedRandom`'s offset. They are baked and marked approximate.

## Work, in order

1. **`motion-expr`** (one pull request):
   - the engine and the determinism layer;
   - the rewrite;
   - the library;
   - the host trait and its prelude;
   - the fitter.

   Tests run on the corpus's idioms: easing scripts, bounces, loops, control links and the legacy forms. A boundary test keeps the engine out of the crates that draw.
2. **The import** (one pull request):
   - model-level evaluation of keys;
   - the host over a decoded project;
   - the bake in place of the keys;
   - the report.

   Measured on s740 and s737 with the ledger.
3. **Layout and noise** (one pull request): `sourceRectAtTime` from our text layout; `wiggle` and the random functions; text selector expressions (`textIndex`). Then the corpus sweep and its numbers in the research note.

## Step 1, as built

- **Corpus:**
  - 1,253 of the 1,260 distinct expressions compile. 13 of them compile after the legacy repair.
  - The other 7 run compiled, encoded scripts and are reported as such.
- **Stack:** the parser and interpreter recurse. A test thread's 2 MB stack overflowed on an ordinary easing script, while 4 MB in a debug build and 2 MB in a release one compile the whole corpus. Engines therefore run on threads with `STACK_SIZE`, 32 MB.
- **No state between frames:** a run leaves nothing behind. The globals it made are removed, and its assignments to the language's names (`position = …`) are cleared, as the original evaluates each frame afresh.
- **Fitter:** two searches, top-down splitting and bottom-up thinning, and the one with fewer keys wins.

  | Curve | Keys |
  |---|---|
  | A decaying bounce, 61 frames, within 0.05 | 18 |
  | A steep exponential ease-out, 31 frames, within 0.05 / 0.1 / 0.25 / 0.5 px | 16 / 14 / 11 / 8 |

  Exponential starts are the hard case for cubics whose ends sit on samples. Letting key values move off the samples is the known improvement, if the key counts matter.
