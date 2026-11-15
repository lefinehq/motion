# Ease families and steps

Curves today are `linear`, `hold`, four CSS béziers (`ease`, `ease-in`, `ease-out`, `ease-in-out`), `cubic(…)` and `speed(…)`. Motion designers and agents reach for the classic families instead: expo, quint, back, elastic and bounce. Our own style profile already writes `ease out-expo, in-out-quint` (`variants.md`), and no scene can parse that yet.

## Design

**Names.** A family's curve is a shape and a family, `<shape>-<family>`:

- **Shapes:**
  - `in` is `f(t)`;
  - `out` is `1 - f(1 - t)`;
  - `in-out` is `f(2t)/2` for `t < .5`, else `1 - f(2 - 2t)/2`;
  - `out-in` is `(1 - f(1 - 2t))/2` for `t < .5`, else `(f(2t - 1) + 1)/2`.
- **Families:**

  | Family | `f(t)` |
  |---|---|
  | `quad`, `cubic`, `quart`, `quint` | `t²`, `t³`, `t⁴`, `t⁵` |
  | `sine` | `1 - cos(tπ/2)` |
  | `circ` | `1 - √(1 - t²)` |
  | `expo` | `(2^(10t) - 1) / 1023` |
  | `back` | `(o + 1)t³ - o t²`, overshoot `o = 1.70158` |
  | `elastic` | a decaying sine with amplitude `a = 1` and period `p = .3`, see below |
  | `bounce` | four parabolas, ending at 1 |

So `out-expo`, `in-out-quint`, `out-back` and `out-in-sine` all parse.

**Parameters.**
- A power of any exponent: `in(2.5)`, `out(3)`, `in-out(1.5)`. The exponent must be above 0.
- Back's overshoot: `out-back(2.5)`, at least 0.
- Elastic's amplitude and period: `out-elastic(1.2,.4)`. The amplitude is at least 1; the period lies in (0, 2].
- A curve whose parameters are its family's defaults prints as the plain name (`in(2)` prints `in-quad`).

**Ends that meet.** Each family is 0 at the start and 1 at the end, with no jump.
- The classic expo, `2^(10t - 10)`, starts at 2⁻¹⁰ rather than 0. Ours is `(2^(10t) - 1)/1023`, which differs by less than 0.1% of the change.
- Elastic decays by the same normalised envelope, `(2^(10t) - 1)/1023` in place of `2^(10t - 10)`. Here `f(t) = -a·E(t)·sin(((1 - t) - s)·2π/p)` with `s = p/2π·asin(1/a)`, which ends at exactly 1.

**Steps.** `steps(n)` holds `n` equal steps and jumps at the end of each: `floor(t·n)/n`, so the last step reaches 1 only when the span ends. `steps(n,start)` jumps at the start of each: `ceil(t·n)/n`. Both match CSS `steps(n, jump-end)` and `steps(n, jump-start)`. `hold` is `steps(1)`.

**Evaluation.** A family or steps curve is evaluated exactly at every frame as `v0 + (v1 - v0)·g(p)` (`Ease::Shaped` and `Ease::Steps` in `motion-scene`). It is not fitted to a cubic. Families that overshoot (back, elastic) go past the end value, as a `cubic` with `y` beyond 1 already does. Along a motion path the move carries on straight past the end, and a property's range still clamps the value.

**Where it shows.**
- **Format:** `Curve::Shaped(Shape, Family)` and `Curve::Steps { count, start }`.
- **Errors:** an unknown curve lists the table names and the family form.
- **Compat corpus:** it uses every shape, every family and `steps` at least once, so the check counts shapes and families rather than all 40 combinations.

**Not in this step.**
- `linear(…)` points: a point list would make `Curve` no longer `Copy`. It comes with its first user, which is storing baked expressions compactly.
- A spring: it gets its own step, where a spring settles in its own time.
- Writing these curves back into a project: the export writes only static values so far. When it writes keys (export step 3), a family curve is baked to keys.

## Tests

- Every family meets both ends exactly.
- Known values: out-expo at .5 is 0.968…, in-out-quint at .25 is 0.015625, out-bounce at .5 is 0.765625, and in-back dips below 0 before .5.
- Shapes are mirror images of each other, in-out is symmetric, and each non-overshooting family never runs backwards.
- Steps jump at the right frames.
- Parsing and printing round-trip.
- Errors name the family form.
- A compat scene draws them (frame hashes).

## As built

Built as designed, with these notes:

- **Behaviours:** `in` tells a curve token by its shape. A token with no digits is still a curve, and so now is a call (`name(…)`), so `pop 18f out-back(2) @6f` reads.
- **Steps at a key:** at the key's own frame the value is the key's value, including for `steps(n,start)`, whose first step CSS shows at once. Every key keeps its value at its time.
- **Compat corpus:** it counts each shape, each family and `steps()`, and `compat/v1/curves.json` uses them all. Its frames are snapshots.
- **Checked with `motion at` on that scene:**
  - `in-quad` at a quarter is 180 of 40 → 600;
  - `out-back(2)` at half is −30 of 600 → 40 (a 12.5% overshoot);
  - `out-bounce` at half is 171.25;
  - `in(2.5)` at half is 138.99;
  - `steps(3)` over 48 frames jumps at frames 16 and 32.
