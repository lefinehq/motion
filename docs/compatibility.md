# Compatibility

nakkash reads files from desktop compositing and editing tools by their file format. This page says what works today and what is planned.

nakkash reads files from desktop compositing and editing tools by their file format. Fidelity is measured against reference renders, sample by sample; nothing here is complete yet.

| Format | State |
|---|---|
| `.aep` projects | **In development.** Layers, shape modifiers, text animators, masks and mattes, many effects, expressions (baked to keys), 2.5D cameras, footage, audio. Measured on 138 public templates: 7 match their reference render on every frame today (within the reference's own compression noise), 5 more once their opening settles. |
| `.mogrt` templates | **In development.** Imported like `.aep`; the template's controls become the scene's slots. |
| Edits back into `.aep` | **In development.** `nakkash export` patches changed values into the original project and lists what cannot go back. |
| Vector footage (`.ai`, `.pdf`, `.eps`) | **Supported**, drawn by nakkash's own renderer, including a built-in PostScript interpreter. |
| `.drp` timelines | **Planned.** Not started. |

What the import cannot do yet, it says: every import prints what it mapped, defaulted, ignored or could not support.

How fidelity is measured: [fidelity ledger](research/fidelity/README.md).
