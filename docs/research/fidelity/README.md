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

- `reference` is a folder of PNG frames. Decode an untagged HD video as BT.709 limited range ([why](../2026-11-14-fidelity-first-sample.md#reading-a-reference-correctly)).
- `ignore` leaves out layers the reference fills with footage we do not have.
- `settled` is when the entrance is over, so the layout's own error is not hidden by the motion's.
- `comp` picks the composition, `start` the scene time of the first reference frame.

## The samples, 2026-11-14

| Sample | Unsupported | Text within | Settled, failing where drawn | Biggest gap |
|---|---|---|---|---|
| s740 | 10 expressions, 1 matte | 2 of 3 | 4.3% | expressions (the first 2 s), then faux bold |
| s737 | 15 expressions, 4 mattes | 2 of 5 | 97.7% | track mattes: the layers used as mattes draw as plain white over the frame, and the title they reveal is hidden |

s737 shows what one missing feature costs: until track mattes are supported, its numbers say little about anything else.
