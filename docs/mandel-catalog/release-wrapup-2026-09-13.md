# Bounded Release Wrap-Up

[Reviewed gallery](../mandel-showcase/README.md) | [Preview-only report](../mandel-release-status/README.md) | [Known-issue groups](../mandel-release-status/known-issues.md) | [All scenes](scenes.md)

This closes the broad screening phase without making universal support claims.
Checkpoint `a65b7dc` remains the renderer/gallery baseline. No rendering code,
scene inputs or existing review decisions changed. Scene **095** was not retried;
fog/cloud fixes remain deferred. No push.

## Release Inventory

| Catalogue status | Scenes | Treatment |
| --- | ---: | --- |
| Reviewed | 423 | Frozen gallery and evidence |
| Experimental | 95 | Reduced native screening plus validated reusable FPT previews |
| Visual holds | 207 | Grouped work queues, not individually fixed |
| Historical execution failures | 21 | One bounded low-cost retest |

All **746 scenes** remain indexed. The preview report uses separate presentation
tiers without rewriting catalogue status. In particular, execution-only successes
remain blocked in the authoritative catalogue until full review is available.

## Reference Recovery

All 95 incomplete cases received one CPU-native screening attempt:

| Result | Scenes |
| --- | ---: |
| Completed reduced reference | 65 |
| Ten-second timeout | 29 |
| Dimension-contract failure | 1 |

- **150px maximum edge**, authored aspect/effects, CPU backend.
- Native Monte Carlo capped at one **only if already enabled**. This does not
  enable MC or imply all native renders use one SPP; other authored AA/adaptive
  settings remain in force. Sampling overrides are recorded per scene.
- Ten seconds per attempt, no repeated retries, zero matching native cache hits.
- Elapsed workflow time: **449.76 seconds (7.5 minutes)**.
- Scene **576** produced 150x150 where the contract expected 150x75. The image
  was rejected, not stretched or silently accepted.

The report reuses **90 geometry and 90 authored FPT captures** at 300px/32 SPP.
Reuse checks include source hash (binding authored camera), exact render command,
dimensions, samples, environment, executable hash, metadata/image hashes and
available lightmap fingerprint. Camera metadata is retained. This is not a complete
audit of every external texture dependency. Missing or invalid captures remain
explicitly unavailable rather than being replaced with unrelated images.

Preview panels label their different source resolutions and sampling. There are
**no cross-resolution MAE rankings or automatic gallery promotions**. Successful
150px references are triage evidence, not replacements for full gallery references.

## Execution Retest

All 21 historical execution failures were retried once in neutral and authored
FPT modes at **96px maximum edge, 1 SPP, 15 seconds per mode**. No native render
was requested for this group. Elapsed time was **31.26 seconds**, with no timeouts.

**471, 521 and 662** now complete both modes at these screening settings. They
are promising review candidates, not certified visual fixes.

Remaining observations:

- **071, 519:** missing authored assets, respectively a user-local photograph
  and an AO lightmap. No substitute texture was invented.
- **447, 456, 457, 458, 513:** authored multi-ray AO rejects iteration-threshold
  distance evaluation. This is a shared unsupported contract, not five independent
  visual bugs.
- **088, 102, 134, 364, 455, 457, 458, 589, 699:** neutral output fails blank or
  single-colour validation. Several authored captures complete; counts overlap
  other failure groups. Successful authored execution does not validate geometry.
- **659:** parser requires an enabled formula in slot one.
- **710:** scene bounds are not finite/increasing at Metal precision.
- **720:** primitive sphere limits are unsupported (reported by the current
  harness under generic execution failure).
- **729:** invalid random light count/seed.

The screening evidence preserves actual status strings and error tails rather
than replacing them with this higher-level grouping.

## Known-Issue Queues

The **207 visual holds** are grouped from existing review notes. Groups overlap
and are suggestions for follow-up, not proven shared causes:

| Queue | Matches |
| --- | ---: |
| Missing illumination / light effects | 150 |
| Geometry control needed | 92 |
| Material / optical effects | 69 |
| Exposure / clipping | 62 |
| Geometry / framing observations | 59 |
| Volume / atmosphere, deferred | 39 |
| Other visual | 3 |
| Scene 095, user-deferred | 1 |

Each entry links back to its reviewed image and original note. Scene 095 appears
only in the user-deferred queue; it was not included in any new capture manifest.

## Budget And Release Boundary

The configured capture ceilings were 95 x 10 seconds for native recovery plus
42 x 15 seconds for FPT retries: **1580 seconds (26.3 minutes)** excluding
orchestration, publication and verification. Actual combined workflow time was
**481.03 seconds (8.0 minutes)**. This is not GPU performance or a controlled
speedup measurement. There were no repeated attempts after render failures within this pass;
an initial native command-path typo failed before capture and was corrected.

The release can include the reviewed gallery, complete catalogue, preview-only
evidence and known limitations without waiting for all outlier fixes. The remaining
work is targeted: recover missing assets/contracts, inspect promising previews at
matching settings, then address shared geometry or surface-lighting failures.
Fog/cloud implementation and scene 095 stay out of scope.

## Verification

All **136 Python tests** passed, including six new wrap-up tests. Reuse tests
reject changed source/settings, renderer identity, environment, camera command,
metadata and capture hashes. Timeout diagnostic images cannot become successful
preview captures. Scene 095 is isolated in the user-deferred queue.

The publication audit verified **144 new artifact hashes**, **1936 frozen
gallery artifact hashes**, **1492 local Markdown links**, all **746 indexed
scenes** and all **116 preview records**. Renderer binaries and the authoritative
catalogue, review decisions, evidence, original gallery and reviewed showcase
remain unchanged. Representative mixed-resolution and missing-image layouts
were visually inspected. Catalogue regeneration and `git diff --check` passed.
No capture processes remain running.
