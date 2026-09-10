# Experimental Mandel Scene Catalogue

[All 746 scenes](scenes.md) | [Catalogue JSON](catalog.json) | [Reviewed showcase](../mandel-showcase/README.md) | [Needs-work audit](../mandel-showcase/needs-work.md) | [Investigation log](../mandel-remaining-scenes.md)

This catalogue makes the complete, deduplicated upstream example collection
addressable by stable scene ID. It adds **metadata and reproducible loading**,
not 746 bundled voxel assets or a claim of universal visual parity.

## Status

| Status | Count | Meaning |
| --- | ---: | --- |
| Reviewed | 83 | Explicit capture-specific visual acceptance, with limitations allowed |
| Experimental | 625 | Opt-in scenes; execution success is not visual acceptance |
| Blocked | 38 | 17 visual needs-work decisions and 21 historical screening failures requiring investigation/retest |

The ranked gallery still contains all 50 rows for transparency, including
blocked scenes **32, 37, 46 and 48**. Of the additional 50, **37** are promoted
and **13** have explicit needs-work decisions. Twelve affected scenes have
fresh FPT captures; the other 38 reuse the reviewed historical 32-SPP captures.
Every scene page labels the capture epoch and renderer identity. The historical
gallery membership field is retained for provenance, not used for promotion.
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

1. Run the support suite on a source-pinned batch at 300px/32 SPP and collect
   matching native CPU references. Reuse references only after hash/dimension
   checks; keep monoscopic or reduced-resolution controls explicitly labelled.
2. Use `scripts/publish_mandel_review.py assemble` to validate all image/metadata
   hashes and create portable evidence plus a local-only capture path map.
   The current pass used the original ranked report, additional-50 report and
   targeted refresh12 report. The `preview` subcommand builds refresh contact sheets.
3. Inspect the images. Record `accepted`, `accepted-with-limitations` or
   `needs-work`, separately assessing geometry and illumination. Every new scene
   requires an explicit annotation. `assessment-2026-09-11.json` is the source
   assessment for this pass, pinned to its evidence-file hash.
4. The `record` subcommand binds annotations to each evidence-row digest.
   Legacy acceptance migrates only when source and all capture hashes match.
   Changed captures require a fresh review, not an automatic acceptance.
5. Rebuild the catalogue, then publish to a fresh output directory using
   `scripts/publish_mandel_review.py publish --evidence ... --reviews ... --assets ... --output ...`.
   Accepted rows get paginated showcase pages; needs-work rows get separate
   audit pages. Keep full RGB detail PNGs and compressed navigation thumbnails
   distinct, and update the README highlights only from accepted rows.

The public catalogue/evidence are portable. The path map and original reports
remain under ignored `reports/`; publication checks the original image hashes
before creating output. Tests reject stale decisions, incomplete evidence,
failed geometry/lighting promotions and changed publication assets.
