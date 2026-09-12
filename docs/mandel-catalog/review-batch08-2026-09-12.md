# Eighth Additional Review Batch

[Reviewed showcase](../mandel-showcase/README.md) | [Needs-work audit](../mandel-showcase/needs-work.md) | [Catalogue](README.md)

Coverage-first publication from checkpoint `adbee4f`. No renderer, source-scene,
camera, shading, sampling or acceptance-policy changes. Known outliers and
incomplete references remain deferred to the consolidated investigation pass.

## Outcome

| Result | Scenes |
| --- | ---: |
| Previously unreviewed scenes selected | 50 |
| FPT neutral geometry succeeded | 49 |
| FPT authored path succeeded | 49 |
| Native CPU references succeeded | 42 |
| Complete native/FPT comparisons | 41 |
| Accepted with limitations | 23 |
| Explicit visual needs-work | 18 |
| Native CPU timeout, unpromoted | 8 |
| Both FPT modes rejected scene bounds, unpromoted | 1 |

The catalogue now contains **263 reviewed, 333 experimental and 150 blocked**
scenes. Blocked includes 129 explicit visual holds and 21 historical screening
failures. The showcase has 27 pages. Prior evidence/decisions for 351 comparisons
and all 702 prior gallery images are retained unchanged.

## Settings And Evidence

- Authored camera/aspect, 300px maximum edge, FPT 32 SPP and existing bounce
  defaults. Chunked single-sample accumulation with eight-row Metal tiles.
- Native CPU at the same dimensions, with authored sampling and effects
  unchanged. No reduced-MC cap, crop, flip or higher-SPP gate.
- Native timeout 120 seconds; FPT timeout 900 seconds. One native CPU reference
  overlaps sequential FPT geometry/authored captures, with a per-scene barrier.
- Source collections: Graeme McLaren 17, Krzysztof Marczak 17, root examples 16.
- Capture checkpoint: `adbee4f63a78e0df99c576d4473a7fdd1e25b5e8`.
- FPT executable SHA256: `698f2967631d743065e1b9101921a52f70e954560ef0aecb6a88b5e17ec294e1`.
- Native executable SHA256: `ecf9de8f749c6c7956573d8c7e7b7ea151562f0db48856d51095f8892071bda8`.
- Upstream revision: `230456cee40968cbaa7f301bba91daa4865a29db`.

[Manual assessment](assessment-batch08-2026-09-12.json),
[capture evidence](review-evidence.json) and [bound decisions](reviews.json)
record source, settings and image identities with per-scene visual notes.
Raw commands, summaries, captures and logs remain in ignored
`reports/mandel-review-batch08-20260912/`.
New evidence uses the actual `captured-2026-09-12` epoch, not the assembler's
inherited refresh label; prior records are unchanged.
Continuous FPT acceptance is not NAADF/CVOX interoperability or parity certification.

## Throughput

The run took **3249.27 seconds (54.2 minutes)**. This is workflow elapsed time,
not a GPU benchmark or a controlled speedup comparison with earlier batches.

There were **zero native cache hits** because these were first-time comparisons.
Eligible successful references populate the provenance-checked cache for later
reuse. No incomplete or unproven historical reference was reused. Native
sampling stayed authored; no lower-resolution or reduced-sample substitutes
were used for publication. Scene 496 completed natively in 113.2 seconds,
inside the unchanged native budget.

Several FPT captures were the slower lane: authored 494 took 164.4 seconds,
167 took 203.5 seconds, 495 took 349.5 seconds and 172 took 515.0 seconds.
Scene 172 also needed 67.5 seconds for its neutral control. The per-scene barrier
can idle the CPU behind expensive FPT work; native timeouts alone no longer
explain total batch duration. No scheduling or renderer changes were attempted.

## Accepted Captures

IDs: **163, 164, 165, 166, 167, 168, 171, 173, 174, 175, 176, 177,
178, 179, 493, 494, 500, 507, 711, 713, 726, 727, 728**.

Open rings, repeated lobes, decorated rounded forms, towers and layered scenes
retain readable geometry and framing. Acceptance allows documented material,
reflection, palette and atmosphere differences where those main forms survive.
It does not certify exact pixels, optical transport, background haze or tiny
filament visibility. Scene 178 is explicitly limited to large-scale structure:
individual subpixel points and their intensities cannot be certified here.

[![Decorated four-tower structure](../mandel-showcase/images/174-thumb.webp)](../mandel-showcase/174.md)
[![Open scalloped ring](../mandel-showcase/images/163-thumb.webp)](../mandel-showcase/163.md)
[![Repeated rounded lobes](../mandel-showcase/images/176-thumb.webp)](../mandel-showcase/176.md)

## Visual Holds

IDs: **169, 170, 492, 496, 497, 498, 499, 504, 715, 716, 717, 718,
719, 721, 722, 723, 724, 725**.

- **169:** native open branching geometry becomes one close surface filling
  the FPT image. A concrete visibility/framing or geometry failure.
- **721:** the native vertical support posts are absent from both FPT controls.
- **170, 496:** dominant forms or open regions differ, with reflective/atmospheric
  effects complicating geometry judgment. They are not accepted as colour changes.
- **715-719:** orbit-trap scenes lose defining luminous curves, beams or lights.
  Geometry is uncertain where native optical effects imply apparent openings;
  corresponding opaque structure alone is not a pass.
- **492, 499:** prominent native light features disappear, including the water
  reflection in 499.
- **497, 504, 725:** loss of illumination makes important surfaces too dark.
- **498, 722, 723, 724:** excessive saturation/clipping obscures native relief
  and illumination gradients. Scene 724's blurred reference also leaves full
  geometry correspondence uncertain.

Images and individual notes are retained in the audit. These related failure
groups are evidence for the later outlier pass, not renderer changes in this batch.

## Deferred Comparisons

Native references **172, 491, 495, 501, 502, 503, 505 and 506** exceeded
120 seconds. Both FPT captures succeeded, but incomplete triplets were not promoted.

Scene **712, nebula 003**, rendered natively but both FPT modes rejected it with:

```text
Error: scene limits must have finite, increasing bounds in Metal precision
```

This is a recorded execution failure, not a GPU crash or a claim of permanent
unsupported status. Its source and mode statuses are preserved in the
[incomplete inventory](incomplete-batch08-2026-09-12.json); raw logs remain local.

All 49 earlier incomplete cases were excluded before selection. These nine
bring the deferred inventory to 58 cases for an explicit later pass. They retain
experimental status. Continue coverage-first review; no push without approval.

## Verification

All 128 Python tests passed; catalogue regeneration and `git diff --check`
passed. Verified 1206 gallery artifact hashes and 4834 local Markdown links.
All 351 prior evidence/decision rows, 702 prior gallery images and the original
ranked 50-scene gallery remain unchanged. Both renderer binaries retain their
recorded hashes. A dry selection excludes all 58 incomplete scenes; no next
capture batch was started and no renderer processes remain running.
