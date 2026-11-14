# Export by patching the project

**Goal:** a scene imported from an `.aep`, then edited, goes back into that project ([D45](../design/decisions.md#d45-export-patches-the-project-it-came-from)). The original tool opens the result with the edits in place and everything else as it was.

**Done means:**
- `motion export <scene> --project <file.aep> --out <file.aep>` writes the project with every edit it can write back, and reports, with the reason, every edit it cannot.
- An unedited scene exports to a file identical to the original, byte for byte.
- The export reads its own output again, imports it, and fails if a written value does not come back as edited.
- The chunk writer reproduces every local project byte for byte, and keeps a file valid when a patch changes a chunk's size.

## Design

- **The chunk writer** (`rifx`): it copies the file, and where a patch replaces a chunk's body, it writes the new body with the new size, then fixes the size of every list around it. Bytes outside patched chunks, pad bytes and the trailing XMP packet included, are copied as they are.
- **Where values come from** (the decoder): a property records the bytes it was read from, the `cdat` body of a static value, as an `Origin`. Byte ranges point into the file the project was decoded from, so an export decodes that same file.
- **Where values go** (the map): wherever the map writes a value from a property that has an origin, it records the scene path it wrote, with the origin and the conversion it used. A value under an expression that is switched on gets no origin: the expression sets it, so writing the stored value would change nothing.
- **The export** (`motion-import::export`):
  1. Import the original again, with the same options, which gives the original scene and the record of origins.
  2. Compare the edited scene with it, value by value. Layers and comps are matched by id, items by index.
  3. Each changed value with an origin is converted back to the project's units (the inverse of the map's conversion, the project's other components kept), encoded as the decoder reads it, and patched in.
  4. Each other change is reported: no origin (a default, a preset, a new key), set by an expression, keyed, or the structure differs.
  5. Decode the patched bytes, import them, and compare every written path with the edited scene.
- **Paths** are lists of keys, ids and indices, local to the export until the layer address (D43, [audit](../research/2026-11-15-architecture-audit.md) workstream 1) gives one type.
- **The pipeline.** The export imports the original as `motion import` does, through the engine. Whether the bake becomes a feature only the CLI turns on (audit workstream 9) does not change the export: it compares like with like, and writes only values the project stores without an expression.

## Work, in order

1. **Static values** (#38): the chunk writer, origins of static values, the map's record, the export, `motion export`. Numbers, points, scales, opacities, colours, effect parameters.
2. **Text:** a text document's text and its runs, rewritten inside the document's object syntax, which changes the chunk's size.
3. **Keys:** a keyed property's key times, values and eases, written back as the decoder reads them.
4. **Controls:** with live control links (D44, audit workstream 5), an edit to a value a control drives goes to the control.

## Step 1, as built

Measured on every local sample project (the ignored test `every_sample_project_writes_back`, 36 s once expressions that never change run once):

- **Unedited:** 238 of 238 projects export byte for byte the same.
- **Every static value edited:** in the 232 projects that import, each of the 15,312 static values the export can write is nudged, written, and read back as edited.
- **Read back in steps.** The import writes numbers to six decimals. A value half a step from both neighbours, such as a colour channel of 0.1177975, rounds up as edited and down after its trip through the file, out of 255. The check allows one step: it failed 10 values of one project before.
