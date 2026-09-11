# Fourth Additional Review Batch

[Reviewed showcase](../mandel-showcase/README.md) | [Needs-work audit](../mandel-showcase/needs-work.md) | [Catalogue](README.md)

Coverage-first publication using the capture-throughput tooling committed
locally as `d328d97`. No renderer, source-scene, camera or shading changes.
Known geometry, lighting and incomplete-reference outliers remain deferred.

## Outcome

| Result | Scenes |
| --- | ---: |
| Previously unreviewed scenes selected | 50 |
| FPT neutral geometry succeeded | 49 |
| FPT authored path succeeded | 50 |
| Native CPU reference succeeded | 43 |
| Complete native/FPT comparisons | 42 |
| Accepted with limitations | 28 |
| Explicit visual needs-work | 14 |
| Incomplete comparisons, unpromoted | 8 |

The catalogue now contains **166 reviewed, 490 experimental and 90 blocked**
scenes. Blocked includes 69 explicit visual holds and 21 historical screening
failures. The showcase has 17 pages. Earlier evidence and decisions for 193
comparisons, and their 386 gallery images, are retained unchanged.

## Settings And Evidence

- Authored camera/aspect, 300px maximum edge, FPT 32 SPP and existing bounce
  defaults. Eight-row Metal tiles and chunked single-sample accumulation.
- Native CPU reference at the same dimensions, with authored sampling and
  effects unchanged. No lower-MC-sample cap and no higher-SPP check.
- Native timeout 120 seconds; FPT timeout 900 seconds. One CPU reference
  overlaps the sequential FPT geometry/authored captures, with a per-scene barrier.
- Source collections: Graeme McLaren 17, Krzysztof Marczak 17, root examples 16.
- Capture tooling revision: `d328d97a7daa4b60fbb9572417e952356ced63d4`.
- FPT executable SHA256: `698f2967631d743065e1b9101921a52f70e954560ef0aecb6a88b5e17ec294e1`.
- Native executable SHA256: `ecf9de8f749c6c7956573d8c7e7b7ea151562f0db48856d51095f8892071bda8`.
- Upstream revision: `230456cee40968cbaa7f301bba91daa4865a29db`.

[Manual assessment](assessment-batch04-2026-09-11.json),
[capture evidence](review-evidence.json) and [bound decisions](reviews.json)
record source/settings/image identities and individual visual reasoning.
Raw commands, captures and logs are retained under ignored
`reports/mandel-review-batch04-20260911/`.
Continuous FPT review does not establish NAADF/CVOX interoperability or parity.

## Throughput

The complete run took **2483.08 seconds (41.4 minutes)**. This is a workflow
observation, not a controlled renderer benchmark or an apples-to-apples speedup
against the previous batch, which contained different scenes.

There were **zero reference-cache hits**: these were first-time comparisons.
Successful eligible captures populate the provenance-checked cache for later
reuse. No legacy reference was reused without a matching complete contract.
The earlier controlled three-scene overlap pilot is documented in
[capture throughput](capture-throughput.md).

The per-scene barrier can still idle the CPU behind a slow FPT capture: scene
641's authored FPT took 417.7 seconds and scene 646 took 180.9 seconds.
No scheduler expansion or renderer fix was attempted in this publication pass.

## Accepted Captures

IDs: **092, 094, 096, 098, 099, 101, 103, 104, 105, 106, 107, 108,
109, 110, 418, 420, 421, 422, 424, 430, 431, 640, 642, 647, 648,
652, 655, 656**.

Acceptance permits recorded colour, reflection, sky and optical differences
where broad geometry, framing and readable illumination remain intact. It is
not exact material, atmosphere, pixel or fine-detail parity.

[![Faceted patterned solid](../mandel-showcase/images/108-thumb.webp)](../mandel-showcase/108.md)
[![Perforated shell](../mandel-showcase/images/110-thumb.webp)](../mandel-showcase/110.md)
[![Nested frames](../mandel-showcase/images/424-thumb.webp)](../mandel-showcase/424.md)

## Visual Holds

IDs: **093, 100, 414, 415, 423, 427, 428, 429, 643, 644, 645,
649, 650, 651**.

Scene **428** is a strong structural failure: native fine nested cavern
openings become broad smooth surfaces even in the neutral FPT control, and
the authored image is dark. Other holds include missing lights/glow,
obscuring darkness, severe clipping, missing checker material and an incorrect
hard horizon strip. Native cloud/transmissive obscuration is marked uncertain
rather than assumed to prove matching geometry. All are deferred, not fixed here.

## Incomplete Comparisons

Native CPU references **097, 416, 419, 432, 641, 646 and 654** timed out at
120 seconds. Scene **417** failed neutral FPT image validation despite successful
native and authored captures. None is promoted or automatically marked unsupported.
See the [source-bound incomplete inventory](incomplete-batch04-2026-09-11.json).

The seven earlier incomplete cases were excluded before selection. Future
batches also exclude these eight until an explicit final outlier pass.
Continue coverage-first review with the same visual gate. No push without approval.

## Verification

All 128 Python tests passed, catalogue regeneration matched, and `git diff
--check` passed. Verified 725 gallery artifact hashes and 2754 local Markdown
links. All 193 prior evidence/decision rows and 386 prior gallery images remain
unchanged, as does the original ranked 50-scene gallery. Both renderer binaries
retain their recorded hashes. The next-batch selection excludes all 15
source-bound incomplete scenes; no next capture run was started.
