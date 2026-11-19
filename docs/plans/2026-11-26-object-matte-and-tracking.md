Status: In progress · 2026-11-19

# Object mattes and tracks from learned models

**Goal:** cut an object out of a clip and track it, from a box drawn on one frame, with models that run on the user's own machine and are licensed for commercial use ([D80 to D88](../design/decisions.md#d80-learned-models-run-in-a-bake-never-in-a-render)). A render never infers: it reads what a bake made.

```json
{"id":"phone","video":"footage/phone.mov","box":[1920,1080],
 "cutout":{"prompts":[{"frame":0,"box":[690,470,1210,735]}],"baked":"3f9a…"},
 "tracks":[{"id":"screen","prompts":[{"frame":0,"box":[700,480,1200,720]}],
            "corners":[[712,488],[1190,492],[1186,716],[708,712]],
            "anchors":[{"frame":36,"corners":[[640,470],[1120,470],[1118,700],[642,702]]}]}]}
```

```sh
nakkash ml models fetch                 # the runtime and the downloadable models, verified
nakkash ml models import ~/exports      # our exported graphs, kept only if the lock has their hash
nakkash ml preview scene.json --out sheet.png   # segment and track, then a contact sheet
nakkash ml bake scene.json              # refine the edges, write `baked` digests into the scene
nakkash ml track scene.json phone/screen --to screen-pivot --corners
nakkash render scene.json --out out.mov # reads the baked matte; never infers
```

## Pull requests

1. **Design records** D80 to D88, the measurements and this plan.
2. **Runtime and models.** `nakkash-ml`: the model lock, the store, fetching and importing with hash checks, the ONNX check of D84, the licence gate; tensors and the graph trait. `nakkash-onnx`: the runtime loaded from the store, sessions placed per graph. `nakkash ml models list | fetch | import | verify` behind the CLI's `ml` feature. A boundary test.
3. **Baked assets in the scene.** `nakkash-baked`: matte and track assets, their manifests, the asset store. `cutout` and `tracks` on video layers; the render multiplies a clip frame's alpha by its baked alpha; a missing or stale bake is an error naming the command. Clip frames by index in `nakkash-video`.
4. **Object mattes.** The segmenter with its memory in Rust, forward and backward from the first prompt; the edge band; the flow smoothing as an option; the work cache; `nakkash ml preview` and `nakkash ml bake`.
5. **Tracks.** Mask tracks, point selection and tracking, the fits and their guard, anchors; the track asset; `nakkash ml track` writing keys on pivots.
6. **Gates.** The ignored gate tests of D86 on the measured shots, and the numbers measured through nakkash on the Mac and the Linux box.

## Open

- Hosting our exported graphs, so that `fetch` can take them too, is the maintainers' call (D82).
- A corner-pin distortion that reads the corner pivots of D88.
- GPU providers on Linux.
