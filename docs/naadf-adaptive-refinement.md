# Adaptive FPT cube refinement — 14 September 2026

Latest result: [adaptive transmission](naadf-adaptive-transmission.md) completes **427/427**
native adaptive execution, with 424 prior captures pixel-identical. Visual
acceptance remains outstanding. The results below describe the earlier stage.

Latest follow-up: [structural-only export recovery](naadf-export-recovery.md) resolves the
two export timeouts. Scene 740 remains the adaptive transmission blocker.

NAADF now consumes static 4×/8×/16×/32× leaves with source colour and authored
material ID for every occupied child. The full catalogue screen renders
**424 of 427 accepted FPT scenes through adaptive cubes**, with **zero black
or uniform adaptive captures**. Scene 740 is translucent and remains outside
the opaque-leaf path. Scenes 380 and 627 retain their existing, identical-
producer Metal compilation timeout evidence. Ordinary cube and exact-surface
modes still execute for all 425 exported scenes.

The six-scene quality pilot improves fine detail, and all five previously
black cube screens recover in fresh paired tests. These are execution and
geometry milestones; no scene has been promoted to clean visual acceptance.

## What changed

The scheduler starts every existing parent at 4×, then promotes the largest
projected child footprint until its 0.25-pixel target or the 16 MiB occupancy
budget is reached. It uses the authored camera and distance to each parent AABB. Perspective
allocation follows the authored FPT ray convention (which differs from the
free camera). Panoramic allocation uses a conservative angular scale and
saturates the polar singularity. The target is an allocation heuristic, not
a formal bound on image error. Ties use sorted parent keys. Parents are never
silently dropped; an insufficient minimum budget fails before conversion.
Empty masks explicitly remove unsupported parent volume. A maximum 32× leaf
and the memory cap can leave the requested screen-space target unmet.

The pinned conservative voxelizer interpolates FPTCOL2 RGB8 vertex colours.
FPTMID1 IDs select the existing authored material upload, retaining roughness,
specular/metallic/reflectance mapping, transmission parameters, IOR, emission
and transport fields while replacing its base colour. Overlapping triangles
use the first source triangle in canonical source order. RGB8 interpolation
is approximate; there is no additional palette quantization. The producer
normalizes numerical alpha rounding because this path requires opaque input.

NAADF traverses occupancy only, then uses the exact child index returned by
DDA to select its material. It does not reconstruct the index from a rounded
hit position or intersect source triangles at runtime. Full-width GPU material
slots preserve large palettes. Legacy uniform masks and parent materials still
work; the extension is negotiated as `fpt_adaptive_materials`.

## Results

All six quality-pilot conversions pass independent all-triangle occupancy
verification and render at 300px, 32 samples, four bounces. Times below include
verification, exclude renderer startup, and come from a two-worker CPU run
except 08; they are observations, not benchmark comparisons. The original
pilot used a 0.125 setting before camera-scale normalization; the corrected
0.25 setting produces byte-identical geometry, occupancy and materials for
all six scenes, verified by their level plans and artifact hashes.

| Scene | Parents | Occupancy MiB | Material MiB | Conversion seconds | Visual observation |
|---|---:|---:|---:|---:|---|
| 03 | 407,641 | 7.78 | 55.34 | 33.95 | 4× satisfies the footprint heuristic; material changes are small; noisy transport remains |
| 04 | 141,251 | 2.69 | 21.48 | 13.19 | 4× satisfies the footprint heuristic; colour detail retained; noisy transport remains |
| 08 | 26,039 | 7.72 | 12.11 | 7.84 | Large foreground blocks substantially reduced; thin stems restored |
| 41 | 7,481 | 7.87 | 21.41 | 1.54 | More small chain-face features and colour variation |
| 475 | 4,130 | 9.91 | 19.94 | 2.09 | Tunnel blockiness substantially reduced; depth/lighting mismatch remains |
| 484 | 2,219 | 1.26 | 2.55 | 0.47 | Non-black, but camera-near surfaces are still conspicuously coarse |

The final full screen uses 192-resolution geometry / 96px / one sample and
four bounces. Every one of the 424 adaptive captures is non-black and
non-uniform; the six panoramic scenes (421, 422, 574, 582, 584, 590) all pass.
The five earlier black cases,
**418, 420, 484, 600 and 703**, all become non-black and non-uniform. Fresh
unrefined controls remain all-black on the same executable, geometry and
capture settings. These five results do not certify visual parity.

The complete gate is in `reports/fpt-adaptive-catalog-final-20260914/merged/`.
It reuses 418 conversions only after proving identical source, converter and
level-plan hashes, and independently verifies six new panoramic conversions.
All render captures in the final gate are fresh. The preceding six-scene
pilot and paired controls are in `reports/fpt-adaptive-20260914/`, with source-bound
records in [naadf-adaptive-results-20260914.json](naadf-adaptive-results-20260914.json).
`review-1.png` and `review-2.png` compare the gallery FPT authored panel,
previous 4× cubes, adaptive children and exact-source surfaces. Gallery images
are qualitative references with different capture settings, not pixel goldens.
`black-controls.png` shows the five fresh paired screens.

## Reproduce

Run the following commands from the native repository at
`/Users/jordantotty/Desktop/vox/metal-voxel-fractal-integration`.

Use the pinned compiler for this checkout, then build both stages:

```sh
RUSTUP_TOOLCHAIN=1.97.1 python3 tools/build_voxel_converter.py
./run_metal_voxel.sh --build-only
python3 tools/adaptive_fpt_bundle.py /path/to/bundle/manifest.json \
  --converter build/voxel-converter/target/release/voxel_refinement_probe \
  --output local-data/adaptive-scene --maximum-axis 300 \
  --budget-mib 16 --target-pixels .25 --verify
python3 tools/view_fpt_bundle.py local-data/adaptive-scene/manifest.json \
  --capture reports/adaptive-scene --diagnostic
```

The ordinary catalogue runner supports `--adaptive-refinement`,
`--refinement-budget-mib`, `--refinement-target-pixels`, `--converter` and
`--verify-refinement`. Uniform `--refine-subdivisions` is mutually exclusive.
The run identity includes the scheduler hash and settings. Source-bound
bundles can be reused with `--bundle-cache`; changed allocation settings cannot
silently reuse a different adaptive bake. A conversion can also be reused
when its exact level-plan, source-bundle and converter hashes match, even if
the planner implementation changed. Reuse is recorded explicitly. Immutable
binary cache assets may be hard-linked; metadata is copied before editing.
A changed plan was tested to force a fresh conversion. The integrated gate covers the complete catalogue, including ordinary cubes,
exact surfaces and adaptive cubes, with 32 Python tests covering its contracts.

Bakes publish by a sibling-directory rename only after hash, size and payload
validation. A failed bake leaves no final bundle. Structural material
validation is cached by content hashes and allowed authored IDs within a
process; file hashes are still checked on every load. The viewer checks material
bindings to geometry and the exact occupancy file and refuses an incompatible
renderer even in diagnostic mode. Surface rendering does not receive child
material or occupancy environment variables.

## Binary contracts and limits

* `NACLEV1\0`: 24-byte header (magic, three u32 dimensions, u32 count), followed
  by sorted `(parent_key, subdivision)` u32 pairs covering every source parent.
* `VQLCUB3\0`: existing 64-byte mixed-occupancy header, now accepting 4/8/16/32;
  sorted `(key, subdivision, absolute_word_offset)` triples, then packed masks.
  Maximum file size is 16 MiB. Disk header word 9 remains reserved zero.
* `VQLMAT1\0`: 32-byte header (magic, version 1, header bytes 32, dimensions,
  parent count); sorted `(key, subdivision, absolute_word_offset, sample_count)`
  u32 entries, then sorted `(child_bit, RGBA8, authored_material_id)` triples.
  Every occupied bit has exactly one sample. Maximum file size is 128 MiB;
  the GPU palette also has a one-million-entry limit. Material exhaustion
  fails explicitly and requires a lower refinement allocation.

The host validates sizes, counts, grids, parent order, contiguous offsets,
occupied bits, coverage, alpha and authored IDs before upload. The runtime-only
material suffix is kept outside the immutable file contract. Existing parent
material tables gain one optional child-table pointer. A temporary high word
in the internal hit coordinate transports `child_bit + 1` through DDA; it is
cleared when the path wrapper resolves the material. Existing non-child paths
retain their original coordinate encoding.

## Verification and remaining work

* Nine Rust producer tests pass, plus a four-level end-to-end two-material /
  gradient fixture verified against independent all-triangle occupancy.
* 32 Python contract/launcher/scheduler tests pass, including failed-bake
  publication, payload binding, allocation and capability rejection.
* C++ loaders pass all four material levels and 69 malformed-payload cases.
* Mixed 4/32, 8/32 and 16/32 native leaves match equivalent uniform 32× masks
  on five ray cases each and a four-bounce path-traced case: identical images
  and normals/material visibility, depth error below 1e-5.
* GPU child-material probe shows 3,447 red and 3,819 green pixels, with
  pixel-identical position output and preserved authored roughness/specular.
* The legacy wide-material full-parent regression remains pixel-identical
  (283 distinct packed materials in this run).

Next: support translucent scene 740, resolve the two producer timeouts,
address camera-near detail beyond the 32× leaf limit, and improve source
coverage and lighting/transport parity. The 96px one-sample catalogue screen
is intentionally a fast failure screen; higher-quality review is still needed. Adaptive material support is currently for opaque perspective and panoramic
bundles. Transmissive and fisheye refinement remain unsupported.
Refinement cannot create uncaptured or parent-excluded source geometry, and
allocation is tied to the authored view. No scene acceptance flags were changed.
