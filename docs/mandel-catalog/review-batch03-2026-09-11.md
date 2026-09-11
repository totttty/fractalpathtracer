# Third Additional Review Batch

[Reviewed showcase](../mandel-showcase/README.md) | [Needs-work audit](../mandel-showcase/needs-work.md) | [Catalogue](README.md)

This is a coverage/publication pass, not an outlier-fix or performance pass.
The 111-scene checkpoint was committed locally as `e8dc689`; this batch adds
new comparisons without changing the renderer or upstream scenes.

## Outcome

| Result | Scenes |
| --- | ---: |
| Previously unreviewed scenes selected | 50 |
| FPT neutral geometry succeeded | 50 |
| FPT authored path succeeded | 50 |
| Complete native/FPT comparisons | 45 |
| Accepted with limitations | 27 |
| Explicit visual needs-work | 18 |
| Native CPU timeout, unpromoted | 5 |

The catalogue now contains **138 reviewed, 532 experimental and 76 blocked**
scenes. The latter includes 55 visual holds and 21 historical screening
failures. The showcase has 14 pages. Previous evidence/decisions for all 148
reviewed cases and all 296 existing gallery images remain unchanged.

## Settings And Evidence

- FPT: authored camera/aspect, **300px maximum edge, 32 SPP**, scene/config
  bounce defaults, chunked single-sample accumulation and eight-row Metal tiles.
- Native reference: Mandelbulber CPU with authored sampling/effects and the
  same target dimensions. Native sampling is not equivalent to FPT SPP.
- No crop, flip, exposure adjustment, source edit, renderer fix or higher-SPP gate.
- Deterministic collection round-robin: Graeme McLaren 16, Krzysztof Marczak 16,
  Robert Pancoast 3 and root examples 15. Only three eligible unreviewed scenes
  remained in the Pancoast collection; the selector continued through the others.
- FPT checkpoint: `e8dc68936c6bcfbe7298cc8226b8bc01d27fa806`.
- FPT executable SHA256: `698f2967631d743065e1b9101921a52f70e954560ef0aecb6a88b5e17ec294e1`.
- Upstream revision: `230456cee40968cbaa7f301bba91daa4865a29db`.
- Batch manifest SHA256: `9f08a490934e30efdb24ae92bcbef7d487d66707f1da1800df565d45f6c139e2`.
- Final raw summary SHA256: `1af32a38fbfee775446aaeca411f46f465c8bc4f6730e68910ed8b521f472904`.

[Manual assessment](assessment-batch03-2026-09-11.json),
[capture evidence](review-evidence.json) and [bound decisions](reviews.json)
record each accepted or held comparison. Raw captures and logs remain in
ignored `reports/mandel-review-batch03-20260911`; runtime capture duration is
not a controlled performance benchmark. Continuous FPT acceptance does not
establish NAADF/CVOX interoperability or visual parity.

## New Accepted Captures

IDs: **075, 077, 078, 079, 080, 081, 082, 083, 084, 085, 086, 087,
089, 090, 091, 398, 400, 406, 410, 412, 587, 588, 590, 626, 627,
628, 629**.

Acceptance allows documented colour, reflection, sky and optical differences
where broad geometry/framing and readable output remain intact. It does not
certify exact materials, atmosphere or fine detail. For example, scene 090's
object geometry aligns but its ground texture differs substantially.

[![Layered green geometry](../mandel-showcase/images/588-thumb.webp)](../mandel-showcase/588.md)
[![Folded structure](../mandel-showcase/images/406-thumb.webp)](../mandel-showcase/406.md)
[![Open spherical lattice](../mandel-showcase/images/087-thumb.webp)](../mandel-showcase/087.md)
[![Curved columns](../mandel-showcase/images/089-thumb.webp)](../mandel-showcase/089.md)

## Deferred Visual Holds

IDs: **076, 399, 401, 402, 403, 409, 411, 413, 618, 619, 621, 622,
623, 624, 625, 637, 638, 639**.

These include missing trap lights/glow, obscuring darkness, clipping, cloud
dependencies and unresolved geometry behind authored atmospheric effects.
Individual notes distinguish known geometry correspondence from uncertainty.
None received a renderer or source-scene fix during this pass.

## Deferred References

Native CPU references for **397, 404, 405, 407 and 620** exceeded the
900-second limit. Their FPT modes succeeded, but partial native outputs were
not accepted as references. See the [source-bound incomplete inventory](incomplete-batch03-2026-09-11.json).

Earlier incomplete scenes **396 and 576** were excluded before selection.
The selector now always excludes checked-in `incomplete-batch*.json` rows,
validating source identities, and supports additional `--deferred` inventories.
This keeps deferred cases experimental without repeatedly selecting them or
misrepresenting a timeout as proof of unsupported geometry.

## Next Pass Policy

Continue through eligible unreviewed scenes using the same visual gate. Keep
all geometry, lighting and reference outliers for a consolidated cleanup pass
after broader catalogue coverage. Do not weaken acceptance or silently retry
deferred scenes. Local commits are allowed; pushing still requires approval.

The disk-space guard paused the run after 409. Its 532 existing capture/log
files were copied to Ventura and hash-verified before resuming at 629. Output
paths changed, but renderer/settings identity and completed captures did not.
Original files were retained; no unrelated files were deleted.
