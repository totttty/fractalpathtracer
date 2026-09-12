# Sixth Additional Review Batch

[Reviewed showcase](../mandel-showcase/README.md) | [Needs-work audit](../mandel-showcase/needs-work.md) | [Catalogue](README.md)

Coverage-first publication from checkpoint `aec19a2`. No renderer, source-scene,
camera, shading, sampling or acceptance-policy changes. Known outliers remain
deferred rather than being retried or fixed during gallery expansion.

## Outcome

| Result | Scenes |
| --- | ---: |
| Previously unreviewed scenes selected | 50 |
| FPT neutral geometry succeeded | 50 |
| FPT authored path succeeded | 50 |
| Complete native/FPT comparisons | 39 |
| Accepted with limitations | 21 |
| Explicit visual needs-work | 18 |
| Native CPU timeout, unpromoted | 11 |

The catalogue now contains **215 reviewed, 410 experimental and 121 blocked**
scenes. Blocked includes 100 explicit visual holds and 21 historical screening
failures. The showcase has 22 pages. Prior evidence/decisions for 276 comparisons
and all 552 prior gallery images are retained unchanged.

## Settings And Evidence

- Authored camera/aspect, 300px maximum edge, FPT 32 SPP and existing bounce
  defaults. Chunked single-sample accumulation with eight-row Metal tiles.
- Native CPU at the same dimensions, with authored sampling and effects
  unchanged. No reduced-MC cap, crop, flip or higher-SPP gate.
- Native timeout 120 seconds; FPT timeout 900 seconds. One native CPU reference
  overlaps sequential FPT geometry/authored captures, with a per-scene barrier.
- Source collections: Graeme McLaren 17, Krzysztof Marczak 17, root examples 16.
- Capture checkpoint: `aec19a2dbb37938d5b0a36b45cfb45024d30a0a4`.
- FPT executable SHA256: `698f2967631d743065e1b9101921a52f70e954560ef0aecb6a88b5e17ec294e1`.
- Native executable SHA256: `ecf9de8f749c6c7956573d8c7e7b7ea151562f0db48856d51095f8892071bda8`.
- Upstream revision: `230456cee40968cbaa7f301bba91daa4865a29db`.

[Manual assessment](assessment-batch06-2026-09-12.json),
[capture evidence](review-evidence.json) and [bound decisions](reviews.json)
record source, settings and image identities alongside per-scene visual notes.
Raw commands, summaries, captures and logs remain in ignored
`reports/mandel-review-batch06-20260912/`.
Continuous FPT acceptance is not NAADF/CVOX interoperability or parity certification.

## Throughput

The run took **2393.41 seconds (39.9 minutes)**. This is workflow elapsed time,
not a renderer GPU benchmark or a controlled speedup comparison with other batches.

There were **zero native cache hits**, as these were first-time comparisons.
Successful eligible captures populate the provenance-checked cache for later
reuse. No incomplete or unproven historical reference was reused. Native
sampling remained authored, including the 116.4-second completed reference
for scene 691; lowering reference quality was not needed to publish this batch.

Some FPT captures remained expensive despite overlap: authored 144 took
157.0 seconds, and 462 took 124.0 seconds. The per-scene barrier can still idle
the CPU behind a GPU capture. No scheduling or renderer changes were made here.

## Accepted Captures

IDs: **128, 130, 131, 132, 133, 135, 136, 137, 138, 140, 141, 142,
143, 144, 145, 468, 472, 682, 688, 690, 691**.

Repeated tubes, open cages, layered shapes, lattice corridors and several
larger fractal structures retain readable geometry and framing. Acceptance
allows documented material/colour, reflection and optical differences where
the broad structure stays clear. It does not certify exact pixels, microscopic
detail, thin-wire intensity, atmosphere or physically equivalent materials.

[![Nested pink and gold spiral](../mandel-showcase/images/682-thumb.webp)](../mandel-showcase/682.md)
[![Orange lattice corridor](../mandel-showcase/images/468-thumb.webp)](../mandel-showcase/468.md)
[![Repeated tube frames](../mandel-showcase/images/133-thumb.webp)](../mandel-showcase/133.md)

## Visual Holds

IDs: **129, 139, 451, 452, 453, 459, 460, 462, 676, 677, 679,
681, 683, 684, 685, 686, 687, 689**.

- **676, 679:** foreground elements correspond, but native open-looking
  background regions become dense structures in FPT. Geometry versus optical
  obscuration remains unresolved; these are not accepted as colour changes.
- **129, 139, 462, 681, 684, 687:** excessive clipping, saturation or lost
  gradients obscure native surface detail. Scene 684 is nearly all white.
- **451, 677, 685, 686:** loss of native illumination substantially darkens
  meaningful structure, despite broadly corresponding geometry.
- **683, 689:** defining cyan beam or cyan/pink orbit-trap lights are missing.
- **452, 453, 459, 460:** fog/glow dependencies obscure the comparison or are
  absent from FPT. Geometry is explicitly uncertain where native obscuration
  prevents a confident judgment.

These failure notes and comparison images are retained in the audit. No fog,
lighting, material or geometry fixes were attempted in this coverage pass.

## Deferred References

Native references **454, 461, 463, 465, 466, 467, 469, 470, 473, 678 and 680**
exceeded 120 seconds. Their FPT modes succeeded, but incomplete triplets were
not promoted. See the [source-bound incomplete inventory](incomplete-batch06-2026-09-12.json).

All 24 earlier incomplete cases were excluded before selection. The next
selector excludes these eleven as well, leaving 35 deferred incomplete scenes
for an explicit later pass. Timeouts remain experimental, not automatically
unsupported. Continue coverage-first review; no push without approval.

## Verification

All 128 Python tests passed; catalogue regeneration and `git diff --check`
passed. Verified 970 gallery artifact hashes and 3784 local Markdown links.
All 276 prior evidence/decision rows, 552 prior gallery images and the original
ranked 50-scene gallery remain unchanged. Both renderer binaries retain their
recorded hashes. A dry selection excludes all 35 incomplete scenes; no next
capture batch was started and no renderer processes remain running.
