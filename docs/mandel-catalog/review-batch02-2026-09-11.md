# Second Additional Review Batch

[Reviewed showcase](../mandel-showcase/README.md) | [Needs-work audit](../mandel-showcase/needs-work.md) | [Catalogue](README.md)

This batch adds visual review coverage, not renderer changes. The scope is
continuous FPT Metal. NAADF/CVOX validation remains separate.

## Outcome

| Result | Scenes |
| --- | ---: |
| Selected, previously unreviewed | 50 |
| FPT neutral geometry succeeded | 50 |
| FPT authored path succeeded | 50 |
| Valid native CPU reference and both FPT captures | 48 |
| Accepted with limitations | 28 |
| Explicit visual needs-work | 20 |
| Incomplete native reference, unpromoted | 2 |

The catalogue now contains **111 reviewed, 577 experimental and 58 blocked**
scenes. Blocked includes 37 explicit visual holds and 21 historical screening
failures. A completed process or low image error never grants visual acceptance.

## Settings And Evidence

- FPT: authored camera/aspect ratio, **300px maximum edge, 32 SPP**, scene/config
  bounce defaults, chunked single-sample accumulation and eight-row Metal tiles.
- Reference: native Mandelbulber CPU renderer, authored sampling and effects,
  matched target dimensions. Native sampling is not equivalent to 32 FPT SPP.
- No crop, flip, exposure correction, replacement reference, or higher-SPP gate.
- Collection-balanced selection: Graeme McLaren 13, Krzysztof Marczak 13,
  Robert Pancoast 12, root examples 12. Previously decided/blocked scenes and
  historical dark/blank warnings were excluded before deterministic selection.
- FPT checkpoint: `88e7081b68a680bd2b9884af385af001608aa902`.
- FPT executable SHA256: `698f2967631d743065e1b9101921a52f70e954560ef0aecb6a88b5e17ec294e1`.
- Upstream revision: `230456cee40968cbaa7f301bba91daa4865a29db`.
- Local batch manifest SHA256: `fb895044416b137dad6d8cd86d9b7c2d298452eecf5629cbb2f7f6b667e04f4f`.
- Local final capture summary SHA256: `c942f664324b4bbbc814d7460f94d6b35d2df1c521c01199eba62010d93fe5e0`.

[Manual assessment](assessment-batch02-2026-09-11.json),
[capture evidence](review-evidence.json), and [bound decisions](reviews.json)
record each source, image, metadata hash and limitation. The prior 100 evidence
and decision rows were preserved unchanged, as were all 200 existing gallery
images. Raw reports, native logs, temporary kernels and local path maps remain
outside Git. These capture wall times are not a renderer performance benchmark.

## Promoted Captures

IDs: **061, 062, 063, 064, 065, 066, 067, 068, 070, 072, 073, 074,
387, 388, 391, 393, 574, 575, 577, 579, 580, 581, 584, 585, 606,
611, 612, 617**.

These preserve readable broad geometry and framing. Some have considerable
palette, reflection, depth-of-field or atmospheric differences, explicitly
noted on their pages; acceptance is not pixel or complete physical parity.

[![Tiered DIFS comparison](../mandel-showcase/images/064-thumb.webp)](../mandel-showcase/064.md)
[![Perforated Menger comparison](../mandel-showcase/images/580-thumb.webp)](../mandel-showcase/580.md)
[![Kleinian comparison](../mandel-showcase/images/074-thumb.webp)](../mandel-showcase/074.md)

## Held Captures

IDs: **069, 384, 385, 386, 389, 390, 392, 394, 395, 578, 583, 586,
604, 605, 607, 608, 610, 613, 615, 616**.

Strongest actionable findings:

- **578**: square openings smear/displace in both FPT modes, not just authored shading.
- **615**: native open boolean cavities become a much more continuous slab;
  authored output is also severely clipped.
- **616**: central boolean partitions/openings need a focused geometry control.
- **384, 385, 389, 392**: authored light/sky separation is lost or greatly reduced.
- **390, 604, 605, 607**: broad highlight clipping hides surface contrast.
- Remaining holds involve obscuring darkness or unresolved optical/background
  correspondence. Per-scene notes distinguish established failures from uncertainty.

This run does not attempt to repair those outliers. Fog/cloud parity and
higher-SPP verification remain deferred.

## Incomplete References

- **576, menger-4D**: native CPU output was 300x300 instead of the expected
  300x150. The strict dimension check rejected it; no stretching or cropping
  was used to hide the mismatch.
- **396, IFS 34**: native CPU reference exceeded the 900-second limit. FPT
  completed, but a partial native render was not treated as evidence.

Both remain experimental, not visually reviewed or proven unsupported.
[Incomplete inventory](incomplete-batch02-2026-09-11.json) preserves their
source identities and mode statuses. A projection-specific reference contract
for 576 and a longer native-only attempt for 396 are possible follow-ups.

## Reproduction

Use the [batch workflow](README.md#review-and-publish-another-batch). This run's
ignored local report directory is `reports/mandel-next50-review-20260911`.
The renderer binary and support-suite harness stayed unchanged during capture.
Disk pressure required moving only this batch's output to another local volume;
the resumed files were hash-verified, and no unrelated artifacts were deleted.

Publishing another batch requires a new manifest and assessment, not reuse of
these decisions on changed captures. No changes from this batch were pushed.
