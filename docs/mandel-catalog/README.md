# Experimental Mandel Scene Catalogue

[All 746 scenes](scenes.md) | [Catalogue JSON](catalog.json) | [Reviewed showcase](../mandel-showcase/README.md) | [Needs-work audit](../mandel-showcase/needs-work.md) | [Investigation log](../mandel-remaining-scenes.md)

This catalogue makes the complete, deduplicated upstream example collection
addressable by stable scene ID. It adds **metadata and reproducible loading**,
not 746 bundled voxel assets or a claim of universal visual parity.

## Status

| Status | Count | Meaning |
| --- | ---: | --- |
| Reviewed | 215 | Explicit capture-specific visual acceptance, with limitations allowed |
| Experimental | 410 | Opt-in scenes; execution success is not visual acceptance |
| Blocked | 121 | 100 visual needs-work decisions and 21 historical screening failures requiring investigation/retest |

The ranked gallery still contains all 50 rows for transparency, including
blocked scenes **32, 37, 46 and 48**. Of the additional 50, **37** are promoted
and **13** have explicit needs-work decisions. Twelve affected scenes have
fresh FPT captures; the other 38 reuse the reviewed historical 32-SPP captures.
Every scene page labels the capture epoch and renderer identity. The historical
gallery membership field is retained for provenance, not used for promotion.
The [second additional batch](review-batch02-2026-09-11.md) attempted another
50 scenes: 48 complete native/FPT comparisons, **28** promoted, **20** held for
visual issues, and two incomplete reference contracts left experimental.
These are fresh 300px-max-edge, 32-SPP FPT captures; no higher-SPP gate was added.
The [third additional batch](review-batch03-2026-09-11.md) adds **27** accepted
comparisons and **18** visual holds. Five native references timed out and remain
experimental. Known outliers and both earlier incomplete references were not
retried; coverage expansion takes priority over fixes.
The [fourth additional batch](review-batch04-2026-09-11.md) adds **28** accepted
comparisons and **14** visual holds. Seven 120-second native timeouts and one
failed FPT neutral capture remain experimental. Bounded CPU/GPU overlap completed
the 50-scene run in 41.4 minutes without lowering gallery reference sampling.
The [fifth additional batch](review-batch05-2026-09-11.md) adds **28** accepted
comparisons and **13** visual holds. All 100 FPT captures succeeded, while nine
native references timed out. The 50-scene run took 46.4 minutes; no renderer,
camera, reference-sampling or acceptance-policy changes were made.
The [sixth additional batch](review-batch06-2026-09-12.md) adds **21** accepted
comparisons and **18** visual holds. All 100 FPT captures succeeded; eleven
native references timed out. Capture time was 39.9 minutes with the same
settings and renderer binaries. Known outliers remain deferred.
Colour differences alone are not a blocker. Geometry, missing assets and
failed render contracts must remain visible. [Explicit decisions](reviews.json)
are tied to the complete [capture evidence](review-evidence.json), including
source hashes, images, settings and reference overrides. A source/capture/settings
change invalidates the decision; it cannot silently inherit an acceptance.

The additional-50 pass used visual comparison rather than MAE as the acceptance
criterion. The original ranked-gallery acceptances were migrated for unchanged
captures, except the four known failures above. This is not a fresh visual review
or current-binary render of every original scene. All rows still leave
NAADF/CVOX validation separate.

The historical remaining-scene screen used **96px max edge, 1 SPP**, with
675/696 scenes passing both FPT modes. Twenty scenes had automatic dark/flat
warnings; flags can overlap failures. These are historical observations from
the recorded binary, not new measurements of today's renderer. Later targeted
water, displacement, interior and sampling fixes do not silently rewrite those
observations. Per-scene review notes also describe their recorded captures;
consult the investigation log for subsequent fixes. No higher-SPP gate has
been added, and no new performance claim is made.

**Every catalogue row leaves NAADF/CVOX validation unestablished.** Continuous
FPT rendering and the downstream voxel-export/NAADF gate are separate. Fog,
clouds, deep-zoom precision and some authored material/lighting behaviour
remain incomplete.

## Use

From the repository root:

```sh
python3 scripts/mandel_catalog.py list
python3 scripts/mandel_catalog.py list --status experimental --search water
python3 scripts/mandel_catalog.py show 572
```

Supply a Mandelbulber2 checkout at upstream revision
`230456cee40968cbaa7f301bba91daa4865a29db`. `MANDELBULBER_ROOT` points to the
directory containing `src` and `deploy/share/mandelbulber2`, not its parent.
The launcher verifies the selected `.fract` SHA256 before rendering. A newer
checkout with changed scene files needs a new reviewed catalogue identity;
there is no ignore-hash option.

```sh
MANDELBULBER_ROOT=/path/to/mandelbulber2/mandelbulber2

python3 scripts/mandel_catalog.py render 01 \
  --mandelbulber-root "$MANDELBULBER_ROOT" \
  --max-axis 300 --samples 32 --out renders/catalog-01
```

Default: continuous SDF, authored path appearance, authored aspect ratio,
300px max edge, 32 SPP, unchanged scene/config bounce settings, chunked
one-sample accumulation and eight-row Metal tiles. `--mode geometry` selects
the existing neutral-material control. There are no automatic camera,
exposure, texture or stereo substitutions. In particular, scene 572's earlier
mono reference override is recorded in its review, not applied to source files.

Use `--dry-run` to print the exact command. Output directories must be new.
Blocked scenes require `--allow-blocked`; it only permits an investigation,
does not upgrade its status, and does not supply missing author-local assets.
The launcher does not download, copy or modify upstream sources. It renders
one explicit scene at a time, never starts the entire corpus implicitly.

## Contents And Attribution

Included in Git: stable IDs, scene paths and hashes, deduplication aliases,
source links pinned to the upstream revision, collection credit/licence
labels, historical mode statuses, warning flags, review notes and evidence
hashes. Collection labels are preserved from upstream paths, not inferred
licences for every individual file. Consult the linked upstream sources for
their terms; this repository does not relicense them.

Excluded: upstream `.fract` copies, author-local textures, generated formula
source/metallibs, voxel volumes, caches, temporary captures and raw reports.
Existing `reports/`, `renders/`, `target/`, `tmp/` and `experiments/` ignore rules
keep runtime artifacts out of normal Git staging. The published ranked-gallery
images remain unchanged; unreviewed screening thumbnails are not added to it.

## Regenerate Metadata

The builder reads the existing two inventory manifests, ranked-gallery
manifest, full historical screening report, additional review manifest and
the checked-in explicit decisions/evidence.
It checks scene identities and completeness and writes no renders:

```sh
python3 scripts/mandel_catalog.py build \
  --screening reports/mandel-remaining696-screen96-20260910/summary.json \
  --additional reports/mandel-additional50-gallery-20260910/manifest.json
```

Append `--check` to detect stale checked-in output without writing. Raw reports
are retained locally rather than bundled in Git; their hashes are in
`catalog.json`. Listing and rendering use only the checked-in catalogue and
do not require those reports. Source-level tests run with:

```sh
python3 -m unittest discover -s scripts -p 'test_mandel_catalog.py'
```

## Review And Publish Another Batch

Current priority: expand reviewed gallery coverage first. Keep geometry,
lighting and incomplete-reference outliers in their recorded audit inventories
for a consolidated cleanup pass after broader coverage. Do not retry them or
weaken the visual acceptance gate merely to increase the reviewed count.

1. Select another source-pinned, collection-balanced batch with
   `python3 scripts/select_mandel_review_batch.py --screening reports/mandel-remaining696-screen96-20260910/summary.json --output reports/next-review/batch.json`.
   This excludes previously decided/non-experimental scenes, historical
   dark/blank warnings, and checked-in `incomplete-batch*.json` inventories.
   Add other source-bound deferrals with repeatable `--deferred` arguments.
   Deferred incomplete scenes keep experimental status; selection is not
   visual acceptance.
2. Run the support suite on that batch at 300px/32 SPP and collect
   matching native CPU references. Native screening now defaults to 120 seconds
   per capture (`--native-timeout`); FPT retains its separate `--timeout 900`
   budget. Pass `--reference-cache reports/mandel-native-reference-cache` to
   reuse fully fingerprinted native references. `--overlap-native` is an
   opt-in CPU/GPU overlap experiment, limited to one capture in each lane.
   See [capture throughput](capture-throughput.md) for safeguards and limitations.
3. Use `scripts/publish_mandel_review.py append-batch --evidence docs/mandel-catalog/review-evidence.json --assets <previous-local-assets.json> --batch <finished-summary.json> --revision <full-renderer-commit> --output <new-evidence-directory>`.
   It validates the finished inventory, source identities and image/metadata
   hashes. Complete triplets are appended; failed triplets go to `incomplete.json`
   and remain unpromoted. The original `assemble` command supports the initial
   ranked/additional/refresh migration; `preview` builds review contact sheets.
4. Inspect the images. Record `accepted`, `accepted-with-limitations` or
   `needs-work`, separately assessing geometry and illumination. Every new scene
   requires an explicit annotation pinned to the new evidence-file hash.
5. The `record` subcommand binds annotations to each evidence-row digest.
   Use `--existing-reviews docs/mandel-catalog/reviews.json` to retain unchanged
   decisions and annotate only the new rows.
   Legacy acceptance migrates only when source and all capture hashes match.
   Changed captures require a fresh review, not an automatic acceptance.
6. Rebuild the catalogue, then publish to a fresh output directory using
   `scripts/publish_mandel_review.py publish --evidence ... --reviews ... --assets ... --output ...`.
   Accepted rows get paginated showcase pages; needs-work rows get separate
   audit pages. Keep full RGB detail PNGs and compressed navigation thumbnails
   distinct, and update the README highlights only from accepted rows.

The public catalogue/evidence are portable. The path map and original reports
remain under ignored `reports/`; publication checks the original image hashes
before creating output. Tests reject stale decisions, incomplete evidence,
failed geometry/lighting promotions and changed publication assets.
