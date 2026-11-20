# Examples

Scenes written from scratch, without footage, brand or real names. Each passes `nakkash check` with nothing found, and a test keeps it so. Agents read them as a guide to the format and to timing ([D72](../docs/design/decisions.md#d72-scenes-written-from-scratch-get-choreography-as-data-and-a-guide)).

| Scene | Size | What it shows |
|---|---|---|
| [`launch.json`](launch.json) | 1920x1080, 4 s | The format's first example: a title and a card that fade up. |
| [`social-916.json`](social-916.json) | 1080x1920, 9 s | A social film. An opening beat readable at frame 0 (`slam`, started 6 frames early), a second beat at 1.2 s, three body beats each a composition pushed in from below and out at the top (`push-up` as `in` and `out` at the same moment), an end card that is complete within 0.4 s and holds 1.4 s. The beats are named with `labels`, and each child enters after its push lands. |
| [`lower-third.json`](lower-third.json) | 1920x1080, 5 s | A name and role on a plate that wipes open from its left edge (`scale-x` about an anchor on that edge, with `mblur`), and leaves the way it came. |
| [`title-card.json`](title-card.json) | 1920x1080, 4 s | A 3D card that swings in (`swing-in`), a title that lands (`slam`), a rule and a subtitle that rises word by word, all fading out together. |
| [`instance/promo.json`](instance/promo.json) | 1920x1080, 6 s | A template an agent fills without reading it ([D91](../docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point)): three sections and typed slots with bounds. The headline may shrink to 72 px in two lines and the CTA may not overflow; colours are read against their pairs. A screen takes a picture or a clip, a choice sets the closing layout, the duration runs from 5 to 8 s, and there is a `light` look. [`instance/promo-de.json`](instance/promo-de.json) is a German instance of it, pinned by the template's hash. |
