# Relative values and value lists

The fifth step of the motion vocabulary, first half; labels and relative times follow in their own step. Two things make a track shorter to write and sturdier to edit.

## Design

**Relative values.** A key's value may be written against the property's resting value on its layer or item:

- `rest`: the resting value itself.
- `rest+<value>` and `rest-<value>`: the rest moved by a value of the property's type, so `rest+40` for a number and `rest+0,-40` for a point.
- `rest*<factor>`: the rest scaled, every component by the same number.

```json
"anim":{"y":"rest+60 -> rest @0-12f out-cubic","scale":"rest*0.8 -> rest @0-12f"}
```

- They work in the string form and in key lists (`{"t":0,"v":"rest+60"}`), on numbers and points. A colour or a path has no relative form, and that is an error.
- The rest is the one the layer has after layout: `position` rests on `at` in its parent's box, so a move written from `rest` follows the layer when it is moved, re-laid out, or framed differently in a variant. Changing `at` is one edit; the track needs none.
- They resolve when the scene compiles. A property's range then clamps the value, as it clamps any value the evaluator samples, since the rest is not always known before then (a text box is measured only when it is set).

**Value lists.** The string form chains values: `"0 -> 200 -> 150 @0-30f out-quad"`.

- The values split the span evenly, and every step takes the curve.
- A value in the middle may say where it falls, as a percentage of the span: `"0 -> 200@40% -> 150 @0-30f"`. The positions run forward, strictly between 0% and 100%, and the values without one share what is left evenly between their neighbours.
- With two values the form is what it was.

## Not in this step

- Labels and relative times (`intro+10f`, `<`, `<<`): the next step.
- Relative values on presets: presets already take `+d` and `-d` from the rest.

## Tests

- Each relative form resolves against the layer's rest after layout, including `at` in a parent's box and a measured text box.
- Key lists take relative values; colours and paths reject them with the reason.
- Value lists place their keys evenly, honour positions, and reject positions that run backwards, sit at the ends, or fall outside the span; two values give the old keys exactly.
- The compat corpus uses both.

## As built

Built as designed, with these notes:

- **`KeyValue`.** A key's value is `KeyValue::Set` or `KeyValue::Rest(Change)` (`Same`, `Plus`, `Times`). Compiling resolves it against the target's rest: the layer's after layout (`position` on `at` in its parent's box, a text's measured box) or the item's own.
- **Ranges.** A set value is range-checked when the scene is read, as before; a relative one is clamped when it is sampled.
- **Positions** are percentages of the span, kept in the span's unit when both ends share one. A chain whose positions run backwards, sit on the ends or are not `<n>%` is an error naming the value as written, and a `->` left with no value after it says so.
- **Checked with `motion at`** on a trial scene: `rest+60 -> rest` on a 100 px rest goes 160 → 100; `0 -> 300@25% -> 100 -> 200 @0-12f` puts 300 at frame 3 and 100 at frame 7.5; a text's `rest-0,40` key starts 40 px above its measured place.
- **Compat corpus:** `relatives.json` chains values with and without positions, and moves from rest on a layer, a child in its parent's box, a text and a shape item.
