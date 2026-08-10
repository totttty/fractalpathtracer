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

## FPTVOX contract version 1

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

Decoders should reject unknown versions or coordinate enum values. Although a
future version may use a larger `header_size`, version 1 requires exactly 64.

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
