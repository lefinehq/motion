Status: Accepted · 2026-11-20

# Templates an agent fills in a few thousand tokens

**Goal:** an agent fills a template, its words, colours, screens and logo, by reading a brief of about 3,000 tokens and writing an instance of a few hundred, never the scene ([D91](../design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point)). Every command that reads a scene reads the instance, merged into its template at one point that refuses what it cannot do, and says so with numbers.

```text
$ nakkash slots promo.json --as-instance > de.json
$ nakkash brief de.json
# nakkash brief · promo.json · sha256:9f3c… · 1920x1080 · 25 fps · 28.04 s
RULES  write only the ids below, in de.json's "slots" · everything else is locked
...
$ nakkash edit de.json '$title-2=Jeder Markt' '$accent-4=#0B6E4F' '$screen={"image":"app-de.png"}'
$ nakkash check de.json
$ nakkash render de.json -o de.mp4
```

## What we measured

On an imported 28 s product promo of the private set (`o200k` tokens, an estimate of a model's count):

| | Bytes (compact) | Share |
|---|---|---|
| Scene | 140,566 (18,606 lines pretty, 56,838 tokens) | 100 % |
| Compositions | 136,553 | 97 % |
| Keyframe tracks, anywhere | 42,900 in 388 tracks | 31 % |
| Shapes | 33,380 | 24 % |
| Effects | 26,274: 436 blocks, 44 distinct; 139 stacks, 24 distinct | 19 % |
| Slots | 1,955 | 1.4 % |

Lossless savings on it, estimated before building them: effect styles 18,882 bytes, named curves 3,356 (38 distinct curves in 362 uses), layer defaults at least 2,151, repeated animation blocks 5,550 (128 blocks, 82 distinct).

On the 198 corpus imports by the current import: 16,155,853 bytes, 651,683 lines, 7,358,588 tokens; per template a median of 3,820 tokens, a 90th percentile of 34,519 and at most 2,665,325; 21 above 30,000. Mask paths are 8.8 MB and shapes 3.0 MB of the 16.2, almost all in the three largest; effects are 0.67 MB (4,230 blocks, styles would save 0.24 MB), keyframes 1.4 MB.

The template has 22 compositions of 15 names in a folder of placeholders, drawn by 22 `comp` layers, each matted by a rounded screen shape: the product screens. The project ships them empty (one holds two pivots that scroll it), so the import made no slot for them. Its colour slots paint: `accent` and `accent-2` only drop shadows (a red and a blue pair), `accent-3` and `accent-5` the corners of a four-colour gradient, `accent-4` only words, `accent-6` and `accent-7` the dark and light of a grade's tint. Six compositions each declare a slot `Tint 1` of the same value.

## Steps

Each is one pull request, merged when the workspace's checks pass. Bit-identical means the same `Scene` after reading and the same frame digests.

| # | What | Bit-identical gate |
|---|---|---|
| 0 | D91 and this plan | — |
| 1 | Equivalence harness: every scene of the compatibility corpus, examples and fixtures written in other forms must read as the same `Scene` and draw the same frames; private folders by `NAKKASH_EQUIVALENT_SCENES` | pretty, compact and key order |
| 2 | Instances: schema, pin and canonical hash, slot patch, `$unset`, `$hide`, `raw`, choices, bounds, looks, `accepts`, the four stages and the problem objects; `render`, `check`, `vary`, `batch`, `export`, `edit`, `slots --as-instance`, `migrate` read them | an empty instance, and one of every template value, against the template |
| 3 | Typed slots at import: placeholder compositions as screen slots, uids, text `overflow`/`min-size`/`max-lines`, colour pairs, file `kind`/`aspect`/`min-px`; `focus` on `cover` | the import corpus: an empty instance against the scene |
| 4 | `nakkash brief`: sections and roles, fit budgets, colours by what they paint, files, choices, locks; text and JSON | — |
| 5 | Compaction: `styles`, named `eases`, defaults left out, `nakkash compact`, the defaults snapshot | every scene compact against itself, and the import corpus |
| 6 | Includes: `nakkash split`, `nakkash flatten`, the canonical hash unchanged by either | every scene split, then flattened |
| 7 | Agent guide, examples and the scripted agent check: an agent filling the template through the brief and an instance against one editing the scene, tokens and check errors | — |

## Follow-up: the fill benchmark

About 50 fill tasks over 3 to 5 imported templates (titles in other languages, a new palette with contrast, screens and a logo, a section chosen, a duration), each run in four arms: the whole scene with a JSON patch, the whole scene rewritten, the brief with `edit`, and the brief with the instance written directly. Scored on the first check passing, locks touched, tokens and round trips, through `nakkash bench`. Planned, not run in this work.
