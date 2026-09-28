# NAADF geometry coverage — 14 September 2026

Latest follow-up: [structural-only export recovery](naadf-export-recovery.md) resolves the
two export timeouts. Scene 740 remains the adaptive transmission blocker.

Later follow-up: [adaptive refinement and subcell materials](naadf-adaptive-refinement.md)
render 424/427 scenes in the full adaptive screen, with no black or uniform
adaptive captures. The evidence below describes the preceding milestone.

The shared bounds fix increases successful catalogue exports from **389 to
425 of 427**. All 36 previous no-triangle failures are resolved. Scenes 380
and 627 still time out; process samples place both in Metal compute pipeline
creation. This is an export/execution milestone, not clean visual acceptance.

## Cause and change

Authored captures trace beyond the default `[-4, 4]` sampling box, but the
fitting function clamped their bounds back into that box. It could construct
an inverted interval when an entire scene lay outside it. Before the fix:

| Scene | Captured finite hits | Hits retained by old bounds | After |
|---|---:|---:|---:|
| 04 | 25,131 | 0 | 25,131 |
| 41 | 29,146 | 22,616 | 29,146 |
| 475 | 50,700 | 14 | 50,700 |

Automatic fitting now encloses the captured extent. Explicit CLI bounds and
clipping remain constrained; disjoint domains fail before reconstruction.
Exports record capture bounds and hit/triangle counts in a geometry sidecar
and the FPTVOX11 bundle manifest. No scene-specific export rules were added.

The regression failed with the old inverted bounds and passes with the fix.
An explicit-bounds scene 41 export remains byte-identical to its baseline;
seven unaffected pilot geometry assets also remain byte-identical.

## Validation and remaining differences

The final 96px/1-sample screen renders all 425 exported scenes in cube and
exact-surface modes. It checks GPU completion and records uniform/all-black
capture flags. Five cube captures need particular attention: **418, 420, 484,
600, 703**. No exact-surface captures are uniform at this screen profile.
Those flags alone do not identify the cause of an image-quality problem.

All 12 pilot scenes export and execute at 300px/32 samples/four bounces in
cube, exact-surface and 4x-refined modes. Visual review confirms that scene
04 is restored, scene 41 includes its distant chain, and scene 475 contains
the tunnel. Coarse geometry and appearance differences remain, and scene
484's cube/refined-cube images are black. No NAADF scene is promoted.

Independent 16x occupancy checks pass for scenes 08, 41 and 475 within the
16 MiB payload limit. They improve detail, but inherit parent materials and
still report 32, 3 and 2 conservative source cells outside parent occupancy,
respectively. Mixed-depth budget scheduling and per-subcell materials remain
the next implementation stage, along with primary-ray reconstruction and
unseen/secondary geometry. Lighting parity remains separate work.

Two earlier surface captures were transiently black despite unchanged source
and renderer identities. Isolated and four-process reruns produced the earlier
successful output; the cause was not established. The consumer now checks
Metal command-buffer completion before reporting/capturing success and supports
negotiated single-sample dispatches for fixed bundle renders. Final captures
use that dispatch mode. No shader code changed.

Validation: **233 Rust tests**, **21 Python contract tests**, and a native
3,783-material full-parent regression with zero changed pixels. The original
FPT catalogue/reviews remain unchanged. The verified native executable is
installed in the consumer's normal local build directory.

## Evidence

Source hashes, report hashes and absolute local evidence paths are recorded in
[the results manifest](naadf-geometry-results-20260914.json). The final catalogue
is in the consumer checkout at
`reports/fpt-geometry-verified-20260914/merged/report.json`. The FPT checkout
contains the pilot, capture/depth probes, visual pages and regression logs in
`reports/naadf-geometry-20260914/`.

- [Representative before/after images](../reports/naadf-geometry-20260914/representative-comparison.png)
- [4x and 16x refinement comparison](../reports/naadf-geometry-20260914/refinement-comparison.png)
- [Final pilot review](../reports/naadf-geometry-20260914/visual-review-final/review.json)
