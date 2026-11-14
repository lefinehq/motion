Status: Living · 2026-11-14

# Fidelity ledger

`ledger.json` holds the latest numbers of every local sample: what its import could not support, how its text lies against the layout the project stored, and how its frames compare with its reference render, over all frames and from when it has settled. Samples are known by id only; the projects, references and fonts stay local (see the [hardening plan](../../plans/2026-11-14-hardening.md#h2-fidelity-ledger)).

```text
motion fidelity samples/manifest.json --ledger docs/research/fidelity/ledger.json
```

prints each sample's numbers and what moved since the ledger, and exits 1 when a number got worse. A pull request that changes fidelity runs it with `--write` and commits the ledger, so its review shows the change as numbers. Moves under 0.0001 are noise and do not count.

The manifest is a JSON file, with paths relative to it:

```json
{"samples":[
 {"id":"s740","project":"projects/740.aep","fonts":["fonts/lato"],"reference":"refs/s740",
  "fps":59.94005994,"ignore":["black-solid-1","image-placeholder"],"settled":3.0}
]}
```

- `fonts` lists font folders, files or locks. A lock (`motion import --fetch-fonts` writes one) needs its files in the font cache.
- `reference` is a folder of PNG frames. Decode an untagged HD video as BT.709 limited range ([why](../2026-11-14-fidelity-first-sample.md#reading-a-reference-correctly)).
- `ignore` leaves out layers the reference fills with footage we do not have.
- `settled` is when the entrance is over, so the layout's own error is not hidden by the motion's.
- `comp` picks the composition, `start` the scene time of the first reference frame.

## The samples

| Sample | Unsupported | Text within | Settled, failing where drawn | Biggest gap |
|---|---|---|---|---|
| s740 | 10 expressions | 2 of 3 (worst 0.59 px) | 3.6% | expressions (the first 2 s) |
| s737 | 15 expressions | 2 of 5 | 97.6% | expressions: the title's colours come from colour controls through expressions, and two elements are timed by them |
| s553 | 9 expressions, 3 masks, 2 strokes, time remapping, a text selector | 1 of 4 (worst 15 px) | 93.4% | masks, and the centring of tracked text (B3); glyphs and widths match exactly with Montserrat 7.200 |
| s611 | 6 expressions, audio spectrum, mirror, drop shadow, an adjustment layer | 1 of 1 | 100% | the audio-driven spectrum and the effects; the text matches exactly |

**A correction.** The first version of this page blamed s737's numbers on its track mattes, saying their layers drew as plain white. That was wrong. The white was the full-frame solid the manifest ignores. Since mattes are supported (track mattes plan), s737's title shows in the right shape and place. But it is white where the reference's is black: its colour comes from a colour control through an expression. s737 now measures expressions more than anything else.
