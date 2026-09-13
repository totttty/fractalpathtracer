# Mandel Release Status

[Reviewed gallery](../mandel-showcase/README.md) | [All 746 scenes](../mandel-catalog/scenes.md) | [Known-issue groups](known-issues.md) | [Screening evidence](screening-evidence.json) | [Complete index](catalog-index.json)

**Reviewed output is frozen. Nothing in this screening report promotes a scene or certifies NAADF/CVOX interoperability.**

## Release Tiers

- Reviewed: 423, unchanged.
- Incomplete comparisons: 95; native reduced-screen outcomes: `{'timeout': 29, 'ok': 65, 'execution_failed': 1}`.
- Existing visual holds: 207; scene 095 remains skipped.
- Historical execution failures: 21 retried; 3 now execute in both modes at screening settings. Their catalogue status remains blocked pending full review.

## Limits

Native previews preserve authored effects at 150px maximum edge; Monte Carlo is capped at one only when already enabled. This is not universally one SPP. Ten seconds per native attempt; no repeated retries.
Execution retests use 96px maximum edge, one FPT sample and 15 seconds per mode. A timeout is not proof of unsupported geometry. These settings are intentionally cheaper than gallery captures.
Reused 300px/32-SPP FPT captures must match source, authored dimensions, exact render command, environment, executable and image/metadata hashes. The source hash binds the authored camera; camera metadata is retained. Lightmaps are checked, but this is not a complete external-texture dependency audit.
There are no cross-resolution MAE rankings, automatic promotions or claims of visual review for these screening previews. Known-issue groups overlap and are work queues, not diagnoses. Fog/cloud fixes and scene 095 remain deferred. Renderer code is unchanged.

## Timing

Native screening: 449.76s. Execution retests: 31.26s. Elapsed workflow time includes overhead and is not GPU performance.

## Previews

- [Preview page 1](preview-01.md)
- [Preview page 2](preview-02.md)
- [Preview page 3](preview-03.md)
- [Preview page 4](preview-04.md)
- [Preview page 5](preview-05.md)
- [Preview page 6](preview-06.md)
- [Preview page 7](preview-07.md)
- [Preview page 8](preview-08.md)
- [Preview page 9](preview-09.md)
- [Preview page 10](preview-10.md)
- [Preview page 11](preview-11.md)
- [Preview page 12](preview-12.md)
- [Preview page 13](preview-13.md)
- [Preview page 14](preview-14.md)
- [Preview page 15](preview-15.md)
- [Preview page 16](preview-16.md)
- [Preview page 17](preview-17.md)
- [Preview page 18](preview-18.md)
- [Preview page 19](preview-19.md)
- [Preview page 20](preview-20.md)
- [Preview page 21](preview-21.md)
- [Preview page 22](preview-22.md)
- [Preview page 23](preview-23.md)
- [Preview page 24](preview-24.md)

## Follow-Up

Use matching-resolution native evidence and explicit visual review before promoting promising previews. Keep reproducible execution errors, geometry issues, surface lighting and deferred volumes in separate work queues. Do not block release of the reviewed gallery on all outliers.
