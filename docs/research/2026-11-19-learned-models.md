Status: Accepted · 2026-11-19

# Learned models for object mattes and tracks: what was measured

Four commercially licensed, full-size models were run through ONNX Runtime 1.31 on the M4 Max Mac and the 16-core EPYC Linux box, before any of [D80 to D88](../design/decisions.md#d80-learned-models-run-in-a-bake-never-in-a-render) was written. The one thing to remember: **a segmenter decides what the object is, and a matting model only refines its edge**; on its own, the matting model cuts out whatever stands out. Every number here is *measured*, on 48-frame 1080p shots, unless it says otherwise.

The models are named by the role they play and their lock key ([D82](../design/decisions.md#d82-a-model-lock-and-a-content-addressed-model-store)); the lock names their upstream sources.

| Role | Lock key | Licence | Size |
|---|---|---|---|
| Video object segmenter: image encoder, two mask decoders, memory encoder, memory attention, our export | `segment.video` | Apache-2.0, weights included | 852 + 18 + 18 + 6 + 28 MB |
| Edge matting model (matting weights) | `matte.edges` | MIT | 973 MB |
| Salient-object matting model (general weights) | `matte.salient` | MIT | 973 MB |
| Point tracker: per-frame features and a tracker over the clip, our export | `points.video` | Apache-2.0, weights included | 115 + 105 MB |
| Optical flow, 12 refinement iterations, our export | `flow.dense` | BSD-3-Clause | 79 MB |

## Shots

- **`product_synth`**: a rendered product that moves 1,170 px, doubles in area and turns about 50°, half covered by a passing ball from frame 22, over real street footage with a skateboarder in it. Exact ground truth: alpha, four face corners, 34 surface points with ray-cast visibility.
- **`man_green`** (hair) and **`cat_green`** (fur): green-screen footage keyed and composited over other footage; ground truth from the key.
- **`watch`, `laptop_screen`, `phone_hands`, `car_snow`, `skater`**: real footage without ground truth, measured by label-free means only.

All footage is under a licence that allows commercial use and changes without attribution. None of it is in the repository.

## Parity

Our export of the segmenter splits it into four graphs, with its memory of past frames kept by the host. Each graph against the upstream module, in eval mode: largest absolute difference 3.1e-4 (image encoder), 1.8e-5 (decoders), 3.1e-6 (memory encoder), 1.8e-5 (memory attention with 7 memories and 16 pointers). The whole video loop on the Linux box, our host loop against the reference video predictor, on the same 48 frames and the same box prompt: **mask IoU 1.000 on every frame**; logits differ by 1.3e-3 at the median frame and 1.9e-2 at the worst, out of a range of about ±20, and the difference does not grow along the clip.

## Speed

Per 1080p frame, wall time; on the Mac with each graph where D81 places it.

| Stage | Mac | Linux box |
|---|---|---|
| Segment and propagate | 0.95 s (encoder 0.42 s on CoreML, memory attention 0.49 s at a full memory) | 17.4 s on 4 cores |
| Edge matting, one 1024² crop | 5.4 s (CPU, quiet machine) to 9 s | 28 s on 2 threads, about 55 CPU-seconds |
| Point features | 0.10 s on CoreML | 2.35 s on 2 threads |
| Point tracker, 34 points over 48 frames | 1.24 s per clip | not measured |
| Flow, forward and backward | 1.06 s on CoreML | about 40 s on 2 threads |

CoreML against the CPU at two threads: image encoder 9×, point features 6.5×, flow 5.8×. The two decoders and the memory attention, whose input shapes change, fail to compile for CoreML (errors −7 and −14). The edge matting model gains nothing (5.8 s on CoreML against 5.4 s on a quiet CPU) and its first compile takes 5 to 19 minutes and 7 GB.

## Determinism

- Repeated runs in one process: bit-identical everywhere.
- One, two and four threads: bit-identical on both machines for every graph, except the point features at four threads on the Mac (5e-7) and the flow at four threads on the box.
- CoreML against the CPU on the Mac: up to 2.1e-3 in the edge model's logits, which moves 40 of 1,048,576 alpha values by one 8-bit level; the segmenter's whole loop keeps a mask IoU of 1.000, its error not compounding through the memory.
- Mac CPU against Linux CPU: a different hash for every graph, by at most 4.4e-3 in logits; mask IoU 1.000, 40 alpha values one level off.

## Mattes

IoU at 0.5 (mean, minimum over frames), SAD in thousands per frame, dtSSD ×1e3 (error of the change between frames against ground truth), and the label-free flow warping error WE ×1e3 in a band around the object.

| Shot | Pipeline | IoU | IoU min | SAD | dtSSD | WE |
|---|---|---|---|---|---|---|
| product_synth | salient matting, whole frame | 0.113 | 0.000 | 448.7 | 194.7 | 38.9 |
| | segmenter mask | **0.989** | **0.949** | **6.2** | 48.1 | 14.9 |
| | segmenter + edge band (D87) | 0.979 | 0.920 | 7.7 | **46.9** | 33.6 |
| | the same + strong flow smoothing | 0.979 | 0.925 | 9.5 | 45.5 | 27.0 |
| man_green | edge matting, whole frame | 0.950 | 0.428 | 18.3 | 30.6 | 6.6 |
| | segmenter mask | 0.988 | **0.985** | 3.8 | 36.4 | 2.0 |
| | segmenter + edge band (D87) | **0.990** | 0.966 | **2.8** | **20.1** | 4.9 |
| | the same + strong flow smoothing | 0.990 | 0.968 | 3.0 | 20.2 | 3.1 |
| cat_green | salient matting, whole frame | 0.996 | 0.995 | 1.1 | 4.4 | 3.1 |
| | segmenter mask | 0.993 | 0.991 | 2.0 | 13.0 | 3.7 |
| | segmenter + edge band (D87) | **0.996** | **0.996** | **1.1** | **3.8** | 3.2 |
| | the same + strong flow smoothing | 0.996 | 0.996 | 1.4 | 4.2 | 2.5 |

The ground truth's own WE is 18.3, 4.7 and 3.7 on the three shots, the floor the flow imposes. Smoothed mattes fall below it on hair and fur, which is lag. On the real footage, the whole-frame matting model agreed with the segmenter on 0.07 (phone in hands: it took the hands too) to 0.98 (skater) of IoU; the edge band was used on 46 of 48 frames of the watch and on none of the 87 × 97 px car.

A wrong prompt is a wasted bake: one shot's first box sat on the palm, and the segmenter tracked the palm faithfully. A contact sheet of the propagated mask caught it, which is why `nakkash ml preview` comes before a bake.

## Tracks

On `product_synth`, against exact ground truth:

| Measure | Result |
|---|---|
| Mask centroid against the visible alpha's centroid | mean 1.4 px, max 5.4 px, through the occlusion |
| Visible points, per-frame median | 0.8 to 2.5 px on frames 0 to 21, 4.7 px at 23, 20 to 37 px from 30 on |
| Corner pin from the raw homography | mean 40.5 px, max 1,291 px |
| Corner pin, guarded | mean 4.8 px over the first half, 21.2 px over all frames |
| Drift after frame 36, anchored at frame 0 only | median 31.2 px (p90 163) |
| The same with a correction at frame 36 | median **2.4 px** (p90 8.9) |
| Points chosen again at frame 12, homographies chained | corner pin mean 12.7 px (21.2 without) |

On the real shots, forward then backward through the clip, points come back within a median of 2.2 to 6.1 px, so the one-way drift is about 2 to 3 px over 48 frames. On a walking person almost no point stays visible, so position comes from the mask. Ramer–Douglas–Peucker reduction at 0.25 px kept 32 to 48 of 48 position keys and 46 to 48 corner-pin keys.

## Licences

Checked on the upstream pages on 2026-11-18: the segmenter and the point tracker are Apache-2.0 with their weights, the matting models MIT, the flow model BSD-3-Clause. Refused, and listed as such in the lock: the leading recurrent video matting model (GPL-3.0); the strongest video matting, video inpainting and fast segmentation models (a non-commercial licence); the leading point tracker (CC-BY-NC-4.0).
