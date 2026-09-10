# Experimental Mandel Scene Catalogue

[All 746 scenes](scenes.md) | [Catalogue JSON](catalog.json) | [Ranked-50 images](../mandel-gallery/README.md) | [Investigation log](../mandel-remaining-scenes.md)

This catalogue makes the complete, deduplicated upstream example collection
addressable by stable scene ID. It adds **metadata and reproducible loading**,
not 746 bundled voxel assets or a claim of universal visual parity.

## Status

| Status | Count | Meaning |
| --- | ---: | --- |
| Reviewed | 48 | Published ranked-gallery scenes, with documented limitations |
| Experimental | 675 | Opt-in scenes; execution success is not visual acceptance |
| Blocked | 23 | Two known ranked-gallery geometry failures and 21 historical screening failures requiring investigation/retest |

The ranked gallery still contains all 50 rows for transparency, including
blocked scenes **46 and 48**. Another 50 scenes have reference-backed review
notes, but remain experimental rather than being automatically promoted.
Colour differences alone are not a blocker. Geometry, missing assets and
failed render contracts must remain visible.

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
manifest, full historical screening report and additional review manifest.
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
