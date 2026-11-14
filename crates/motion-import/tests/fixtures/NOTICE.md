# Test fixtures: sources and licences

The `.aep` projects and Lottie exports in this folder are third-party files, copied unchanged for testing the reader. They are not covered by this repository's licence; each keeps its own.

| Files | Source | Licence |
|---|---|---|
| `BPC-8.aep`, `BPC-16.aep`, `BPC-32.aep`, `ExEn-es.aep`, `ExEn-js.aep`, `Item-01.aep`, `Layer-01.aep`, `Property-01.aep` | [boltframe/aftereffects-aep-parser](https://github.com/boltframe/aftereffects-aep-parser), its `data/` folder | MIT, Copyright (c) boltframe |
| `Benchmark.aep`, `DynamicGradient.aep`, `HamburgerArrow.aep`, `Masks.aep`, `Miter_Limit.aep`, `RGBMarker.aep`, `Skew.aep`, `Star.aep`, `TrimPathsInsideAndOutsideGroup.aep` | [airbnb/lottie-android](https://github.com/airbnb/lottie-android), its folder of sample projects | Apache-2.0, Copyright Airbnb, Inc. |
| `lottie/*.json` | The Lottie exports of those projects, from [airbnb/lottie-android](https://github.com/airbnb/lottie-android); `Benchmark.json` and `Miter_Limit.json` are renamed from `benchmark.json` and `MiterLimit.json` to match their projects | Apache-2.0, Copyright Airbnb, Inc. |

The Apache-2.0 text is in [`LICENSE-APACHE`](../../../../LICENSE-APACHE) at the root of this repository.

Never add a project whose licence forbids redistribution. Such projects are read only by the local sweep in `tests/sweep.rs`, from `$MOTION_AEP_SAMPLES`.
