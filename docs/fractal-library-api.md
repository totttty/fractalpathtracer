# Fractal library and voxel-export contract

FPT Metal exposes a Rust library for scene parsing, deterministic reference
voxelization, exact voxel payload interchange, `.fptvox`, and GLB export. Real
Mandelbulber `.fract` files use the CLI adapter because their authoritative
evaluator is the existing generated Metal program, not a second CPU port.

## Rust API

The package builds both the `fpt-metal` binary and the `fpt_metal` library.
The stable data seam is:

```rust
pub fn voxelize(request: &VoxelizationRequest)
    -> Result<VoxelGrid, FractalError>;

pub fn export_glb(grid: &VoxelGrid, path: impl AsRef<Path>)
    -> Result<GlbExportSummary, FractalError>;

pub fn export_fptvox(grid: &VoxelGrid, path: impl AsRef<Path>)
    -> Result<FptvoxExportSummary, FractalError>;

pub fn export_fptvox_with_normals(
    grid: &VoxelGrid,
    packed_normals: &[u32],
    path: impl AsRef<Path>,
) -> Result<FptvoxExportSummary, FractalError>;

pub fn export_fptvox_with_planes(
    grid: &VoxelGrid,
    packed_planes: &[u32],
    path: impl AsRef<Path>,
) -> Result<FptvoxExportSummary, FractalError>;

pub fn voxelize_to_glb(
    request: &VoxelizationRequest,
    path: impl AsRef<Path>,
) -> Result<(VoxelGrid, GlbExportSummary), FractalError>;

impl VoxelGrid {
    pub fn from_dense_cells(
        resolution: [u32; 3],
        bounds: Aabb,
        coordinate_system: CoordinateSystem,
        source_label: impl Into<String>,
        source_sha256: impl Into<String>,
        cells: &[VoxelCell],
    ) -> Result<Self, FractalError>;
}
```

`Aabb`, `FractalScene`, `VoxelizationRequest`, `VoxelizationParameters`,
`SurfaceMaterial`, `VoxelCell`, `SparseVoxel`, `VoxelGrid`, and
`FractalError` derive `Serialize`/`Deserialize`. `FractalErrorCode` is a stable
machine-readable category. `MandelbulberScene` and its public parsed formula,
gradient, and material records are re-exported for consumers which need scene
inspection.

`voxelize` is deliberately a deterministic CPU/reference path for built-in
fixtures. It currently implements `BuiltinFractal::MengerSponge`, samples cell
centres in X-fastest order, and applies the legacy half-diagonal surface-band
rule. A `FractalScene::Mandelbulber` request returns
`CpuReferenceUnavailable`; use the production CLI below. This avoids silently
substituting a partial CPU evaluator for generated Mandelbulber formulas.

## Exact voxel payload

`VoxelCell` is `#[repr(C)]` and exactly 12 bytes, matching `Shaders.metal` and
the Objective-C++ bridge:

```text
u32 packed_color       RGB UNORM8 in bits 0..23, occupied in bit 31
u32 packed_properties  roughness, specular, transmission,
                       (ior - 1) / 1.5 as four UNORM8 channels
f32 emission           unquantized emission strength
```

`VoxelGrid::from_dense_cells` accepts the Metal kernel's X-fastest dense
buffer and returns occupied cells sorted by linear index. `to_le_bytes` and
`from_le_bytes` provide a stable portable representation of an individual
cell. Material deduplication always compares the two packed words and the
exact emission float bits.

## Production `.fract` export

```sh
cargo build --release

target/release/fpt-metal voxel-export scene.fract \
  --out scene.fptvox \
  --voxel-resolution 256 \
  --mandelbulber-root /path/to/mandelbulber2
```

Optional controls are `--bounds-min x,y,z`, `--bounds-max x,y,z`,
`--surface-band 0.25..4`, and `--fill-interior`. Mandelbulber defaults to a
normalized FPT/glTF-space `[-4, 4]^3` box. Its generated Metal evaluator uses
FPT Metal's internal similarity scale while sampling, then records the
normalized bounds in the artifact. JSON FPT scenes use their configured voxel
bounds. A
deterministic no-external-source fixture is also available as
`builtin:menger-sponge`.

For `.fract`, the command:

1. Parses the scene with the existing importer and generates the same
   scene-specialized Metal formula source used by rendering.
2. Retains and caches `voxel_build_kernel` and its dependencies.
3. Dispatches that kernel into a shared dense buffer and reads back every
   exact 12-byte `VoxelCell`.
4. Converts occupied cells to `VoxelGrid` without changing their payloads.
5. Writes either the lossless sparse volume or, for `.glb`, greedy-meshes
   exposed faces whose packed material tuples are identical. The choice is
   inferred from the output suffix.

The dense readback costs `12 * N^3` bytes (192 MiB at 256 cubed) before
sparsification. The command fails rather than writing an empty mesh, which
makes incorrect bounds visible to automation.

## FPTVOX contracts

`.fptvox` is the preferred direct volume seam. It preserves every occupied
cell's exact packed colour/occupancy, PBR properties, and emission without
greedy meshing or glTF material conversion. All integers and IEEE-754 float
bit patterns are little-endian.

The version-1 header is exactly 64 bytes and has no reserved fields:

| Offset | Size | Field | Encoding |
| ---: | ---: | --- | --- |
| 0 | 8 | `magic` | ASCII `FPTVOX1` followed by NUL |
| 8 | 4 | `header_size` | `u32`, exactly 64 in version 1 |
| 12 | 4 | `version` | `u32`, exactly 1 |
| 16 | 4 | `resolution_x` | `u32` |
| 20 | 4 | `resolution_y` | `u32` |
| 24 | 4 | `resolution_z` | `u32` |
| 28 | 4 | `coordinate_system` | `u32` enum |
| 32 | 4 | `bounds_min_x` | `f32` |
| 36 | 4 | `bounds_min_y` | `f32` |
| 40 | 4 | `bounds_min_z` | `f32` |
| 44 | 4 | `bounds_max_x` | `f32` |
| 48 | 4 | `bounds_max_y` | `f32` |
| 52 | 4 | `bounds_max_z` | `f32` |
| 56 | 8 | `voxel_count` | `u64` |

Coordinate-system enum values are:

| Value | Meaning |
| ---: | --- |
| 1 | Right-handed, positive Y is up; the production CLI emits this value |
| 2 | Right-handed Mandelbulber source coordinates, positive Z is up |

Records begin at byte 64. There are exactly `voxel_count` records, each 24
bytes:

| Record offset | Size | Field |
| ---: | ---: | --- |
| 0 | 4 | cell X, `u32` |
| 4 | 4 | cell Y, `u32` |
| 8 | 4 | cell Z, `u32` |
| 12 | 4 | packed RGB plus occupancy, `u32` |
| 16 | 4 | packed roughness/specular/transmission/IOR, `u32` |
| 20 | 4 | emission, `f32` |

Records are strictly ordered by
`x + y * resolution_x + z * resolution_x * resolution_y`, matching
`VoxelGrid`. Export rejects invalid coordinates, duplicates, out-of-order
records, impossible counts, and non-finite payloads before creating a file.
Version 1 intentionally omits source labels and hashes: it is a geometry and
material interchange contract, not a scene manifest.

The intended native path is:

```text
FPT scene / Mandelbulber .fract
    -> authoritative Metal voxel_build_kernel
    -> VoxelGrid
    -> .fptvox
    -> native NAADF volume construction/traversal/path tracing
```

Decoders should reject unknown versions or coordinate enum values. All current
versions require `header_size` 64.

### Experimental surface payloads

`voxel-export` keeps version 1 as its default. Two explicit flags select
structural surface payloads generated by the authoritative Metal evaluator:

```sh
# FPTVOX2: occupancy/material record plus a smooth normal.
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 192 --surface-normals

# FPTVOX3: locally clipped tangent-plane surface cells.
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 192 --surface-planes
```

Both variants retain the same 64-byte header and append one `u32` to each
version-1 record, making each sparse record 28 bytes:

| Version | Magic | Final record word |
| ---: | --- | --- |
| 2 | `FPTVOX2\0` | octahedral normal: low `u16` X, high `u16` Y |
| 3 | `FPTVOX3\0` | local tangent plane: 12-bit oct X, 12-bit oct Y, 8-bit signed offset |

The public `export_fptvox_with_plane_pairs` API writes experimental version 5.
Its 32-byte record appends two version-3 plane words at offsets 24 and 28. The
first word is required; zero in the second word means one surface patch. A
consumer intersects both planes with the current voxel cell and selects the
nearest valid ray intersection. Version 4 was a rejected patch-mask experiment
and is intentionally unsupported.

For version 3, the offset byte maps linearly from `[0, 255]` to `[-1, 1]`
voxel units along the decoded normal from the cell center. The exporter samples
the cell center and corners, projects the closest finite distance estimate to
the surface, and keeps only cells whose projected tangent plane intersects the
cell. The exact packed material tuple is retained for every kept cell.

Versions 2, 3, and 5 are opt-in experiments. They are intended for native NAADF
structural comparison, not as replacements for version 1's lossless occupied
volume contract. Version 3 improves visible surface detail for several tested
fractal classes, but sparse high-complexity surfaces can cost more to traverse
than the conservative full-block representation. Version 5 currently has a
library export seam rather than a `voxel-export` flag because production
camera-independent surface clustering is still under validation.

`--surface-patches` writes experimental version 6 (`FPTVOX6\0`). Each 36-byte
record retains the 24-byte material cell and appends a V3 primary plane, an
optional secondary plane, and four dominant-axis projection bounds for the
secondary patch: `min_u`, `max_u`, `min_v`, and `max_v`, each quantized to one
byte. The first plane is required; zero in the second plane slot means one
patch and zero bounds.

The Metal exporter first applies the established V3 surface-cell gate. It then
projects a deterministic 3x3x3 probe lattice onto the implicit surface,
clusters at most two sufficiently supported normal orientations, and derives
sampled projected bounds. The secondary is omitted when its fitted plane lies
more than 0.2 cell units from the voxel center. This follows the same
dominant-axis principle used by conservative triangle voxelizers, but does not
convert the fractal to a mesh. Consumers first intersect the unbounded V3
primary plane and only test the bounded secondary when the primary has no valid
cell intersection.

V6 additionally lets four or more agreeing probe samples establish a primary
plane when the legacy primary fit fails. The Metal kernel tags these cells with
an exporter-private bit that is removed before `VoxelGrid` construction. The
CPU retains the tagged cells only when all candidate surface cells cover at
least 90% of the dense grid; otherwise it removes them and reproduces the
established sparse V6 path. The JSON report exposes candidate occupancy,
promotion count, and the selector result. This density specialization is an
offline export cost and does not affect V1, V3, or ordinary rendering.
V6 is an opt-in structural experiment; it does not change V1, V3, or V5.

### Coherent camera-independent completion

`--surface-complex-patches` runs a wider five-word analysis kernel but compacts
the accepted result back to the standard three-word FPTVOX6 payload before
serialization. Existing V6 cells keep their primary plane and optional bounded
secondary exactly as generated by `--surface-patches`. For the first weaker
probe-support tier, the exporter accepts only cells that:

- fit one coherent normal group rather than two competing patches;
- produce a valid bounded primary patch; and
- have at least ten stronger retained cells in their 3x3x3 neighborhood.

The neighborhood is evaluated against the stronger set only, so completion
cannot grow transitively. Accepted minimum-tier cells are written as a
non-intersecting primary sentinel plus one bounded patch. The consumer therefore
uses the existing FPTVOX6 bounded-secondary path without a format, shader, or
runtime-buffer change.

```sh
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 192 --surface-band 0.35 \
  --surface-complex-patches --surface-promotion-min-probes 21 \
  --mandelbulber-root "$MANDELBULBER_ROOT"
```

On the held-out Greek view this changed IoU from `0.732713` to `0.734517`,
mean/median/p95 position error from `5.2909/1.7057/21.3934` to
`5.1797/1.6075/21.2657` voxels, and normal absolute-cosine mean from `0.5791`
to `0.5862`, with zero extra pixels. A BoxFold control was metric-identical.
The wider analysis increases offline export work but does not affect NAADF
render-time code or resources. It can change the ordinary V6 acceleration
structure by adding cells. Four alternating 160-frame pairs measured Greek at
`1.930 -> 1.885 ms` (`-2.3%`) and BoxFold at `1.987 -> 2.018 ms`; BoxFold's
paired median delta was `+0.57%`, within the `1% / 0.05 ms` gate.

### Local-parallax surface completion

`--surface-local-parallax` is an optional offline completion pass for V6:

```sh
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 192 --surface-band 0.35 \
  --surface-patches --surface-promotion-min-probes 22 \
  --surface-local-parallax \
  --mandelbulber-root "$MANDELBULBER_ROOT"
```

The pass keeps the authored camera as a held-out view. It traces twelve
continuous-SDF training views on two deterministic parallax rings, three and
six voxel cells from the authored camera. For each sampled cell it averages
aligned continuous normals, fits a local plane and dominant-axis bounds, and:

- replaces only the optional secondary patch on an existing V6 record;
- adds a new bounded record only when it is directly adjacent to the original
  V6 surface;
- copies material data from the nearest original neighboring cell;
- leaves the existing primary plane, traversal contract, and consumer format
  unchanged.

The accepted quality configuration remains the default. A controlled cost
sweep can override it without changing the base V6 path:

```sh
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 192 --surface-band 0.35 --surface-patches \
  --surface-local-parallax \
  --surface-local-parallax-views 6 \
  --surface-local-parallax-resolution 256 \
  --surface-local-parallax-rings 2 \
  --mandelbulber-root "$MANDELBULBER_ROOT"
```

Supported view counts are 4, 6, and 12; sampling resolutions are 192, 256,
and 384; and ring count is one or two. View count must divide evenly across
the rings. A single ring is placed 4.5 voxel cells from the authored camera;
the validated two-ring path remains at three and six cells. Export JSON records
the effective view, resolution, and ring values. Lower settings are
experimental until they pass the same held-out structural gate.

The initial cost sweep found that `6/256/1` reduced diagnostic GPU time from
about 140 seconds to 24 seconds on Greek while improving its held-out IoU,
median position error, and normal agreement. It was not a universal quality
win: on a current-exporter BoxFold control it improved normal agreement but
slightly reduced IoU and position accuracy. Consequently, lower-cost settings
remain explicit experiments rather than replacing the `12/384/2` quality
default.

The one-cell adjacency rule is important. Ungated global-orbit sampling filled
more rays but introduced unrelated fractal layers in front of the held-out
camera, substantially worsening depth. The local bounded policy was retained
because it improved all hit-position statistics and normal agreement on three
geometrically different held-out scenes:

| Scene, 192^3 | IoU before | IoU after | Misses before | Misses after | Mean position before/after | Normal abs cosine before/after |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| asurfKlein/difsGreek | 0.7327 | **0.7426** | 39,413 | **37,961** | 5.291 / **4.356** | 0.579 / **0.708** |
| BoxFoldBulb v3 | 0.4659 | **0.4672** | 76,128 | **75,938** | 37.356 / **37.193** | 0.473 / **0.489** |
| AmazIfs Torus | 0.9662 | **0.9688** | 1,385 | **940** | 14.886 / **14.374** | 0.576 / **0.600** |

This mode is intended for cached final assets. The continuous training captures
can dominate export time (about 140 seconds for the Greek validation scene on
an Apple M1 Max). They do not run in the NAADF consumer and therefore add no
per-frame renderer cost. Without the flag, the Greek default V6 artifact
remains byte-identical to the pre-feature exporter output.

## GLB contract version 1

Consumers must read `asset.extras.fpt_voxel_contract`. Version 1 includes:

```json
{
  "name": "fpt_voxel_grid",
  "version": 1,
  "resolution": [256, 256, 256],
  "bounds_min": [-4.0, -4.0, -4.0],
  "bounds_max": [4.0, 4.0, 4.0],
  "occupied_cells": 1234,
  "surface_quads": 5678
}
```

The marker also records source identity, coordinate systems, cell layout, and
backward-compatible aliases. A valid production artifact has version 1 and a
nonempty mesh.

PBR values use `KHR_materials_specular`, `KHR_materials_transmission`,
`KHR_materials_ior`, and, when needed, `KHR_materials_emissive_strength`.
Core glTF cannot express the Metal payload perfectly, so every glTF material
also stores the authoritative packed tuple at
`material.extras.fpt_voxel_cell`. The visible GLB mesh is a boundary proxy;
the in-memory `VoxelGrid` remains the exact sparse volume.

## Consumer integration

For built-ins or constructed volumes, depend on this repository as a normal
Rust path/git dependency and call the in-memory API. For generated `.fract`
formulas, invoke the versioned `voxel-export` process seam on macOS and ingest
`.fptvox` directly into the downstream native volume. GLB remains available
for mesh-oriented consumers. The CLI is the current stable adapter until
generated Metal compilation and device ownership can be exposed without
coupling consumers to `FptRenderConfig`.

## Licensing boundary

FPT Metal's hand-written Rust, bridge, voxel contract, and GLB encoder are
Apache-2.0. Mandelbulber2 is GPLv3-or-later. The importer reads an external
checkout and generated formula source/metallibs stay in ignored runtime cache
directories; they are not distributed as part of the Apache-2.0 library.
Consumers are responsible for the license and redistribution terms of their
Mandelbulber checkout, `.fract` files, and any generated artifacts they ship.
