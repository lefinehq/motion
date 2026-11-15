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

- `fonts` lists font folders, files or locks. A lock (`motion import --fetch-fonts` writes one) needs its files in the font cache. A font the project names by another name of a face given here is that face ([D61](../../design/decisions.md#d61-a-font-is-found-by-the-names-its-face-gives-itself)) and is not counted. A font not given is set in a substitute ([D40](../../design/decisions.md#d40-missing-fonts-are-substituted-at-import)), and each use counts as unsupported, so the gap stays measured.
- `reference` is a folder of PNG frames. Decode an untagged HD video as BT.709 limited range ([why](../2026-11-14-fidelity-first-sample.md#reading-a-reference-correctly)). Take exact frames of the video: every k-th one (`select=not(mod(n\,k))`), with `fps` the video's rate over k, such as 23.976/12. ffmpeg's `fps` filter keeps the last frame of each slot, so at 2 a second from 25 its frame for 0.5 s shows 0.74 s; on the local corpus, references taken that way made every entrance fail.
- `ignore` leaves out layers the reference fills with footage we do not have.
- `settled` is when the entrance is over, so the layout's own error is not hidden by the motion's.
- `comp` picks the composition, `start` the scene time of the first reference frame.
- `excluded` says why a sample cannot pass however well motion draws it, such as a reference rendered from other footage than the project names. It is still measured, so a change to it shows in the ledger, but it is left out of the score.

## The score

A sample passes when every compared frame passes the fidelity limit of [Quality](../../design/quality.md#fidelity): no more than 1% of its content past 0.016 per channel (`DiffOptions::PASSING`, `Error::passes`). Each window counts its passing frames (`frames_passing`, higher is better in the ledger) and its empty ones (`frames_empty`): frames where the scene draws nothing and the reference is its background. Those pass without showing anything, so a window passes only when one of its frames has content, and a window with no frame never does. Means can fall a long way without a sample passing, so `motion fidelity` ends with the score, sample by sample (`Scoreboard`, `Verdict`):

```text
score: 1 of 181 pass every frame, 1 more once settled (13 excluded, 0 not yet scored)
  pass (1): m759
  pass once settled (1): m562
  fail with nothing unsupported (11): m628 m632 m643 m650 m737 …
  fail with one gap (26):
    video footage in H.264 (drawn as nothing) (7): m777 m785 m789 …
    `ADBE Vector Shape - Star` (3): m625 m778 m801
    …
  fail with more gaps: 142
  excluded m576: the reference lays the transition over stock footage; …
```

That is the local corpus of 198 samples on 2026-11-15; 4 more could not be measured.

- **pass**: every frame passes. **pass once settled**: every frame from `settled` passes, but not all of the entrance.
- **fail with nothing unsupported**: the import named no gap, so something is drawn wrong or lost without a word. These come first: each is a bug, not a missing feature.
- **fail with one gap**: one feature away from the limit, grouped by that feature, the one blocking the most first. Closing it is the quickest way to move the score.

## The windows

Each frame's error is measured over three sets of pixels ([D42](../../design/decisions.md#d42-fidelity-is-headlined-over-the-reference-content)):

- **`content`**, the headline: where the scene draws, and where the reference differs from the scene's background by more than the tolerance (0.016 per channel). Content the scene fails to draw is measured here, and fails. Where an ignored layer draws and nothing else does, the reference is left out, as the layer is.
- **`drawn`**: where the scene draws in that frame. Low `drawn` with high `content` means content is missing, not misdrawn.
- **`union`**: everything the scene draws in any compared frame, so an element that arrives late counts.

Before `content`, a scene that drew nothing in its settled frames scored a perfect 0: in the local corpus of 198 templates, 31 of 39 passing samples passed only that way. The four samples below all ignore a full-frame layer the reference fills, so for them `content` equals `drawn`.

## The samples

| Sample | Unsupported | Text within | Settled, failing over the content | Biggest gap |
|---|---|---|---|---|
| s740 | 10 expressions | 2 of 3 (worst 0.59 px) | 3.6% | expressions (the first 2 s) |
| s737 | 15 expressions | 2 of 5 | 97.6% | expressions: the title's colours come from colour controls through expressions, and two elements are timed by them |
| s553 | 9 expressions, 3 masks, 2 strokes, time remapping, a text selector | 1 of 4 (worst 15 px) | 93.4% | masks, and the centring of tracked text (B3); glyphs and widths match exactly with Montserrat 7.200 |
| s611 | 6 expressions, audio spectrum, mirror, drop shadow, an adjustment layer | 1 of 1 | 100% | the audio-driven spectrum and the effects; the text matches exactly |
| s562 | stars, zigzag, drop shadows, motion blur, footage it cannot read (the logo) | 0 of 1 (a substituted font) | 0.27% | the logo and the stars; its lines draw on with trims (9), which took it from 69.6% to 4.2% failing over all frames |

**A correction.** The first version of this page blamed s737's numbers on its track mattes, saying their layers drew as plain white. That was wrong. The white was the full-frame solid the manifest ignores. Since mattes are supported (track mattes plan), s737's title shows in the right shape and place. But it is white where the reference's is black: its colour comes from a colour control through an expression. s737 now measures expressions more than anything else.
