# Fractal library and voxel-export contract

FPT Metal exposes a Rust library for scene parsing, deterministic reference
voxelization, exact voxel payload interchange, and GLB export. Real
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
  --out scene.glb \
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
4. Converts occupied cells to `VoxelGrid` and greedy-meshes exposed faces.
   Faces merge only when their packed material tuples are identical.
5. Writes a glTF 2.0 GLB with one primitive per deduplicated material.

The dense readback costs `12 * N^3` bytes (192 MiB at 256 cubed) before
sparsification. The command fails rather than writing an empty mesh, which
makes incorrect bounds visible to automation.

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
formulas, invoke the versioned `voxel-export` process seam on macOS, validate
the contract marker, then load the GLB in the downstream renderer. The CLI is
the current stable adapter until generated Metal compilation and device
ownership can be exposed without coupling consumers to `FptRenderConfig`.

## Licensing boundary

FPT Metal's hand-written Rust, bridge, voxel contract, and GLB encoder are
Apache-2.0. Mandelbulber2 is GPLv3-or-later. The importer reads an external
checkout and generated formula source/metallibs stay in ignored runtime cache
directories; they are not distributed as part of the Apache-2.0 library.
Consumers are responsible for the license and redistribution terms of their
Mandelbulber checkout, `.fract` files, and any generated artifacts they ship.
