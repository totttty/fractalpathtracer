# Fifth Additional Review Batch

[Reviewed showcase](../mandel-showcase/README.md) | [Needs-work audit](../mandel-showcase/needs-work.md) | [Catalogue](README.md)

Coverage-first publication from checkpoint `35c5aa0`, with the existing
timeout/cache/bounded-overlap capture workflow. No renderer, source-scene,
camera, shading or sampling changes. Known outliers remain deferred.

## Outcome

| Result | Scenes |
| --- | ---: |
| Previously unreviewed scenes selected | 50 |
| FPT neutral geometry succeeded | 50 |
| FPT authored path succeeded | 50 |
| Complete native/FPT comparisons | 41 |
| Accepted with limitations | 28 |
| Explicit visual needs-work | 13 |
| Native CPU timeout, unpromoted | 9 |

The catalogue now contains **194 reviewed, 449 experimental and 103 blocked**
scenes. Blocked includes 82 explicit visual holds and 21 historical screening
failures. The showcase has 20 pages. Prior evidence/decisions for 235 comparisons
and all 470 prior gallery images are retained unchanged.

## Settings And Evidence

- Authored camera/aspect, 300px maximum edge, FPT 32 SPP and existing bounce
  defaults. Chunked single-sample accumulation with eight-row Metal tiles.
- Native CPU references at the same dimensions, with authored sampling and
  effects unchanged. No reduced-MC cap, crop, flip or higher-SPP gate.
- Native timeout 120 seconds; FPT timeout 900 seconds. One native CPU capture
  overlaps sequential FPT geometry/authored captures, with a per-scene barrier.
- Source collections: Graeme McLaren 17, Krzysztof Marczak 17, root examples 16.
- Capture checkpoint: `35c5aa020e1fb61aa4a946c894c7fea8ccbe2253`.
- FPT executable SHA256: `698f2967631d743065e1b9101921a52f70e954560ef0aecb6a88b5e17ec294e1`.
- Native executable SHA256: `ecf9de8f749c6c7956573d8c7e7b7ea151562f0db48856d51095f8892071bda8`.
- Upstream revision: `230456cee40968cbaa7f301bba91daa4865a29db`.

[Manual assessment](assessment-batch05-2026-09-11.json),
[capture evidence](review-evidence.json) and [bound decisions](reviews.json)
record source, settings and image identities with per-scene visual reasoning.
Raw commands, summaries, PNGs and logs remain in ignored
`reports/mandel-review-batch05-20260911/`.
Continuous FPT acceptance is not NAADF/CVOX interoperability or parity certification.

## Throughput

The complete run took **2784.90 seconds (46.4 minutes)**. Different scenes
prevent a controlled speedup comparison against previous batches. This is
capture-workflow elapsed time, not renderer GPU benchmark time.

There were **zero native cache hits**: these are first-time comparisons. The
validated cache retains successful eligible references for later reuse; it
does not make previously unseen scenes faster. Native timeouts are not cached
as successful references and no unproven legacy capture was reused.

FPT itself limited throughput on some scenes: authored captures took 196.2 s
for 442, 178.4 s for 444 and 175.8 s for 123. The last had a native reference
of only 5.6 s. Bounded overlap therefore cannot remove all idle CPU time.
No scheduler redesign or performance optimization was attempted in this batch.

## Accepted Captures

IDs: **111, 112, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124,
125, 126, 127, 433, 438, 448, 661, 664, 665, 667, 668, 669,
671, 672, 673, 675**.

Wire lattices, helices, checker geometry and repeated angular structures are
strong examples. Some reflective/fractal scenes retain readable broad geometry
despite major colour, atmosphere and reflectance differences. Each such limit
is explicit; acceptance does not certify exact pixels, microscopic detail,
native optical effects or physically equivalent materials.

[![Open spherical lattice](../mandel-showcase/images/122-thumb.webp)](../mandel-showcase/122.md)
[![Block corridor](../mandel-showcase/images/121-thumb.webp)](../mandel-showcase/121.md)
[![Cut shells and connector](../mandel-showcase/images/127-thumb.webp)](../mandel-showcase/127.md)

## Visual Holds

IDs: **113, 114, 434, 435, 440, 444, 445, 446, 657, 658, 660,
666, 670**.

- **445:** the native central sky opening is filled with nested structures in
  FPT, including the neutral control. A strong structural/visibility outlier.
- **660, 670, 444:** substantial visible-region differences with unresolved
  material, optical or structural causes; no confident geometry acceptance.
- **434, 666:** large clipped or washed-out regions obscure useful detail.
- **113, 114:** loss of defining native procedural material patterns, not just
  hue differences. Scene 113 also loses surface contrast in nearly uniform red.
- **435, 446, 657, 658:** missing environmental/projected illumination or
  obscuring darkness. **440:** native blur/veil prevents confident comparison.

None was fixed during this pass. The evidence remains available in the audit,
separate from the accepted showcase.

## Deferred References

Native references **436, 437, 439, 441, 442, 443, 449, 450 and 674** exceeded
120 seconds. All their FPT captures succeeded, but incomplete triplets are not
promoted. See the [source-bound incomplete inventory](incomplete-batch05-2026-09-11.json).

All 15 earlier incomplete cases were excluded before selection. The next
selector excludes these nine as well, leaving 24 deferred incomplete scenes
for an explicit later pass. Timeouts remain experimental, not automatically
unsupported. Continue coverage-first review without retrying known outliers;
no push without approval.

## Verification

All 128 Python tests passed; catalogue regeneration and `git diff --check`
passed. Verified 851 gallery artifact hashes and 3293 local Markdown links.
All 235 previous evidence/decision rows, 470 existing gallery images and the
original ranked 50-scene gallery remain unchanged. Both renderer binaries
retain their recorded hashes. A dry selection excludes all 24 incomplete
scenes; no further capture batch was started.
