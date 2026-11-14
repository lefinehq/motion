# Font resolver

**Goal:** a scene, or a project being imported, gets its fonts with one command. The command resolves each font to an exact file from an open-licensed source, picks the version by evidence where a project gives some, and pins it in a lock file. Renders stay offline and repeatable ([D12](../design/decisions.md#d12-what-repeatable-means), [D33](../design/decisions.md#d33-text-is-shaped-from-font-assets)): they read only the lock file and a local cache, and never reach the network.

**Done means:**
- `motion fonts fetch scene.json` writes `scene.fonts.lock` next to the scene and puts the files in the cache. The scene lists the lock in `fonts`.
- `motion import project.aep --fetch-fonts` does the same for an imported scene. Where the project stores a font's layout, it picks the version whose glyphs and widths match that layout, and reports the score.
- A scene whose lock names a file missing from the cache, or one whose bytes do not match its hash, fails with a message that names the command to run.
- Nothing that renders links a network client; a test fails if one does.
- The fidelity ledger grows from 2 samples to every local sample whose fonts are open-licensed.

## Why versions matter, measured

The project files store the glyph ids of every laid-out line. For one sample, set in Montserrat ExtraBold and Medium:

| Candidate | Glyph ids of `Mixkit` | Matches the project |
|---|---|---|
| Stored in the project | 101 270 359 284 270 327 | |
| v3.100, upstream OTF | 101 270 359 284 270 327 | yes, and widths within 0.01 px |
| v5.001, v6.002 (OTF and TTF) | 192–195 … | no |
| v7.200–7.222 (OTF and TTF) | 117–120 … | no |
| Google Fonts v4.000–v9.000 (static and variable) | 120–195 … | no |

So "the latest Montserrat" would have been wrong, and so would "the Google Fonts Montserrat": the template was made with a release Google Fonts never shipped. The glyph ids tell exactly which one it was. Sources therefore have to include each family's own repository, and the choice has to be made on evidence.

## Design

- **Crate `motion-fonts`.**
  - Offline by default: the lock file format, the cache and the check that a file is the one locked.
  - Behind the `fetch` feature: the sources, the network client, candidate listing and scoring. `motion-engine` uses the offline part; only `motion-cli` turns on `fetch`.
- **Sources, as data** (`sources.json` in the crate). Each family names its licence and where its releases live:
  - Google Fonts' repository: the commits that touched the family's folder.
  - The family's own repository: its tags, with the paths its releases used.

  A family not listed falls back to Google Fonts' `ofl/`, `apache/` and `ufl/` folders. Only those licences are fetched; a font with another licence is reported, never fetched.
- **Candidates.**
  - For each source, list the refs (one API call per source) and try each release's file for the PostScript name by its raw URL. Raw files are not rate limited, and an immutable ref's file is fetched once.
  - Every file goes into the cache by its SHA-256. It must parse as a font and contain the PostScript name asked for, as a face or as a variable font's named instance.
- **Choice.**
  - With stored layouts (an import): each candidate is scored by the share of stored glyph ids its character map gives, with capitals applied, then by how close its plain advances come to the stored widths. The best wins, and the report prints the score.
  - Without layouts: the newest release, or the one `--version` names.
- **Lock file** (`<scene>.fonts.lock`, JSON). One entry per PostScript name: family, version string, licence, source repository, ref and path, SHA-256 and size. Its order is stable, so diffs read well.
- **Rendering.**
  - `fonts` in a scene may name a lock file. The session loads each locked file from the cache, at `$MOTION_CACHE` or the platform's cache folder, and checks its hash.
  - A missing or changed file is an error naming `motion fonts fetch`. The session never fetches.

## Work, in order

1. The plan and decision D39 (fonts are resolved before a render, never during one).
2. `motion-fonts`, offline: the lock, the cache, verification. The engine loads locked fonts. Unit tests with the fixture font.
3. `motion-fonts`, `fetch`: the sources, candidate listing, the cache and scoring against stored layouts. Scoring is unit-tested on fixture data; the network is tested only by an ignored test.
4. CLI: `motion fonts fetch`, and `motion import --fetch-fonts`.
5. The boundary test: no rendering crate links a network client.
6. Measure: the samples with open fonts join the manifest; ledger before and after.

## What changed while building

- **Locks are named after their scene**, `story.fonts.lock` beside `story.json`. Scenes that share a folder then never change each other's fonts.
- **Downloads come from GitHub's raw files** at the tag or commit. jsDelivr gives each tag's file list with SHA-256s, so a file already in the cache is not downloaded again. It will not list a repository over 50 MB, and for those tags GitHub's tree of the tag stands in. Raw files' entity tags follow their content, so an unchanged file is fetched once across commits.
- **GitHub's API** (60 requests an hour without a token) is used only for tags, commits and the large trees, and its answers are reused for a day. `GITHUB_TOKEN` or `GH_TOKEN` is sent when set.
- **Releases are read lazily**, and a release's files are downloaded together. Without evidence, the search stops at the first release with every face. With evidence, it stops at an exact match, or where a run of releases that set every stored glyph ends: glyph numbering changes between releases, so a run is the versions the project could have been made with, and widths pick among them.
- **Variable fonts can name their instances differently from the static fonts.** Google Fonts' Open Sans calls its bold `OpenSansRoman-Bold`, while projects ask for `OpenSans-Bold`. The family's older static files live in `apache/opensans`, which is now one of its sources.

## Result

| Sample | Family | Chosen | Candidates | Stored glyphs matched | Cold / warm |
|---|---|---|---|---|---|
| s604 | Montserrat | JulietaUla/Montserrat v3.100, OTF | 29 | 237 of 237 | 17 s / 0.4 s |
| s553 | Montserrat, Lato | v7.200; Google Fonts | 17; 2 | 12 of 12; 15 of 15 | 6 s |
| s611 | Roboto | Google Fonts, 2026-04 | 3 | 23 of 23, every line exact | 8 s |
| s622 | Open Sans | Google Fonts `apache/opensans`, 2017 | 2 | 42 of 42 | 15 s |

- Every local sample with open fonts now resolves to a file whose glyphs match the project's on every stored character. Where lines are still off, the cause is known and not the font: tracked centred text (B3) in s553, box text placement in s622.
- s553 and s611 joined the fidelity ledger with locks. s604 waits for a substitute for Arial (licensed), and s622 for a reference at the scene's size.
- Without evidence, a plain scene (Lato and Roboto) resolves in 10 s cold.
