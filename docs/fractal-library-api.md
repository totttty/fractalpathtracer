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

### Authored appearance extensions

Mandelbulber exact-surface exports append independent, versioned trailers after
the geometry payload. Geometry versions 1 through 11 remain unchanged. A
consumer may stop after geometry or validate the following sequence:

1. `FPTAPP1\0`: fixed 256-byte background, authored lights, image adjustments,
   and compatibility material controls.
2. `FPTCAM1\0`: fixed 96-byte camera pose, roll, adjusted field of view,
   image-Y convention, and perspective/fisheye/equirectangular projection.
3. `FPTENV2\0`: fixed 512-byte environment header followed by a portable
   128x64 RGB16F HDRI lookup texture. The header stores basic, volumetric, and
   iteration-fog controls plus cloud density, placement, motion, color, and
   noise parameters.
4. `FPTMAT1\0`: a fixed 32-byte header and 68-byte records for every authored
   `matN`, including base color, roughness, specular response, metallic,
   reflectance, transmission, interior opacity, IOR, emission, and
   transmission tint.
5. `FPTCOL2\0`: indexed-triangle vertex colors used for barycentric shading.
6. `FPTMID1\0`: one authored, nonzero `matN` identifier per indexed source
   triangle. The 32-byte header declares version `1`, a 4-byte record, and an
   exact triangle count.
7. `FPTNRM1\0`: one optional packed shading normal per indexed source
   triangle. The 32-byte header declares version `1`, a 4-byte record, and an
   exact triangle count. Bit 30 marks a present record, bits 0-29 store three
   unsigned-normalized 10-bit XYZ components, and bit 31 is reserved.

Every numeric field is little-endian and finite. Each extension has its own
magic, version, record sizes, and strict count validation. Old geometry-only
artifacts remain valid and deterministic.

The environment trailer embeds decoded image data rather than an absolute HDRI
path, so the artifact remains portable. It preserves the authored volume
controls, but a consumer may implement iteration fog or procedural clouds with
an explicitly documented approximation when the voxel payload lacks the
original distance-estimator orbit state.

`FPTENV2` replaces the original 32x16 `FPTENV1` lookup because the smaller
payload visibly erased authored sky detail. Geometry and all other trailer
contracts are unchanged; native readers retain support for both versions.

### Indexed surface-colour extension

FPTVOX8 and FPTVOX11 exports use `FPTCOL2\0`, which stores three packed RGB8
values per indexed source triangle. The renderer interpolates them with the
actual hit barycentrics rather than selecting a flat triangle or averaged cell
color. The 32-byte header declares version `2`, a 12-byte record, and an exact
triangle count. Each color word is `0x00BBGGRR`; high bytes are reserved.

Readers retain compatibility with the earlier `FPTCOL1\0` four-byte flat-color
record by expanding it to three equal vertices. Malformed counts, reserved
bits, and unknown versions are rejected rather than silently shifting later
extensions.

The stream is deliberately separate from the canonical geometry payload. It
does not change traversal, occupancy, normals, or the existing per-cell
material table, and older consumers can continue to stop after geometry.

### Indexed triangle-material extension

FPTVOX8 and FPTVOX11 exports append `FPTMID1\0` after the triangle-color
stream. The material record follows the same source-triangle ordering as
`FPTCOL2`; consumers therefore select the hit triangle's authored material
without changing geometry or barycentric color interpolation. Export rejects
zero identifiers and triangles whose three structural samples disagree about
their material.

The selected formula material is also marked in its `FPTMAT1` flags. This
avoids assuming that `mat1` is the formula material when the source scene uses
`formula_material_id`. The remaining material-appearance limitation is image
texture evaluation: authored scalar properties and generated palette colors
are preserved, but Mandelbulber color, normal, and displacement texture graphs
are not embedded in the current portable trailer.

### Indexed triangle-normal extension

FPTVOX8 and FPTVOX11 exports append `FPTNRM1\0` after `FPTMID1\0`. A zero
record requests the triangle's geometric normal. A present record restores the
continuous source normal for camera-facing fallback splats, so surface coverage
geometry does not incorrectly control diffuse or specular shading. Connected
triangles normalize and sign-align their three source vertex normals, average
them, and emit an override only when that average differs by more than five
degrees from the geometric normal of the final quantized triangle. The gate is
evaluated in world-space bounds so anisotropic grids do not bias the decision.

The native NAADF consumer validates the lossless 10-bit-per-axis stream, then
repacks it into the unused upper 16 bits of its hot triangle word as a 5/5/5
normal plus a presence bit. This replaces a side-buffer lookup with data from
an already-required triangle load. The current runtime quantization measured
`2.61` degrees p95 on the splat-dominated scene-3 gate and `2.71` degrees p95
on the fully disconnected scene-4 gate.

Across the first five authored scenes, selective connected overrides changed
zero hit pixels and reduced mean continuous-normal error from
`2.33/34.01/29.82/46.22/22.79` degrees to
`1.42/13.19/13.19/1.95/15.15`. They selected
`5.5/88.1/34.9/99.6/44.5%` of triangles. Three alternating full 64-frame Metal
batches measured `+2.76/+1.75/+2.70/+2.34/-12.07%` GPU time relative to
geometric normals. This is an opt-in appearance-correctness cost; artifacts
without `FPTNRM1` are unaffected. A three-normal barycentric `FPTNRM2`
prototype reduced scene-2 mean error to `10.02` degrees and scene-4 to `1.95`
degrees, but was rejected because it regressed four required scenes by
approximately `3-16%`. FPTNRM1 deliberately remains scalar per triangle.

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

### Exact triangle surfaces (FPTVOX7)

`--surface-triangles` writes a replacement-style surface representation rather
than another fitted-plane side table. The producer samples topology in Metal,
runs marching cubes and cell clipping in Rust, then samples packed materials in
Metal only for the occupied output cells. The native NAADF consumer traverses
the same Direct16 occupancy hierarchy but resolves exact triangles inside each
candidate cell.

The 96-byte little-endian header is:

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 8 | `FPTVOX7\0` |
| 8 | 4 | header size, exactly 96 |
| 12 | 4 | version, exactly 7 |
| 16 | 12 | output resolution X/Y/Z |
| 28 | 4 | coordinate system |
| 32 | 24 | bounds min/max as six `f32` values |
| 56 | 8 | occupied cell count |
| 64 | 12 | topology sampling resolution X/Y/Z |
| 76 | 4 | cell record size, exactly 32 |
| 80 | 8 | triangle count |
| 88 | 4 | triangle record size, exactly 12 |
| 92 | 4 | reserved, zero |

Each sorted 32-byte cell record stores XYZ, the existing 12-byte packed
material tuple, a full `u32` first-triangle index, and a full `u32` triangle
count. The consumer uploads first-triangle, triangle-count, and 32-bit material
index planes before the triangle stream. The material plane keeps V7 exact when
a scene deduplicates to more than the 32,767 materials representable inside a
conventional NAADF Direct16 voxel record; the NAADF hierarchy carries occupancy
only for V7 and the exact triangle hit supplies the material. Each 12-byte
triangle stores three `u32` vertices.
Within a vertex, bits 0..9, 10..19, and 20..29 are cell-local X/Y/Z UNORM;
bits 30..31 are reserved and zero.

The native consumer uses its monolithic transmissive kernel for V7. The
optional experimental split-glass path is rejected at startup because that
legacy specialization does not bind the V7 exact surface/material payload.

```sh
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 96 \
  --surface-triangles --surface-triangle-resolution 192
```

#### Optional finite-surface bounds retry

`--surface-triangle-auto-bounds` enables one conservative retry when the first
surface build contains occupied cells on any of the six output-grid faces.
`--surface-triangle-auto-bounds-margin` selects the fractional expansion on
each side and accepts values from `0.001` through `1.0` (default `0.10`):

```sh
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 96 \
  --surface-triangles --surface-triangle-resolution 192 \
  --surface-triangle-auto-bounds \
  --surface-triangle-auto-bounds-margin 0.10
```

The exporter expands all three world-space bounds symmetrically and increases
both output and topology grids by the same factor. This preserves output-cell
and topology-sample size instead of lowering spatial resolution. Output grids
are capped at 512 cells per axis and topology grids at 511 samples per axis;
the report records the smaller effective margin when either cap is reached.

The retry replaces the original build only if all six candidate boundary-cell
counts are zero. A candidate that still touches a boundary is rejected and the
original volume is written unchanged. This makes the option useful for finite,
accidentally clipped objects without attempting to infer a finite extent for
unbounded or intentionally framed fractals.

The JSON report includes `auto_bounds.requested`, `attempted`, `accepted`, the
initial and candidate bounds and resolutions, total and per-face boundary-cell
counts in X-/X+/Y-/Y+/Z-/Z+ order, the effective margin, and a stable reason.
Without the flag, FPTVOX7 export behavior is unchanged.

The default `--surface-triangle-threshold-scale 1.0` preserves the
Mandelbulber mesh-export threshold. A non-default scale is a view-matching
experiment for a fixed continuous-render camera: Mandelbulber's interactive
distance estimator uses a camera-distance-dependent acceptance band, while a
triangle volume must encode one view-independent surface. Consequently, a
non-default scale changes the extracted surface and is not valid for PLY
structural-parity claims. The selected value is recorded as
`topology_threshold_scale` in the export report.

Mandelbulber scenes that declare `legacy_coordinate_system true` retain its
legacy image-plane Y convention when rendered directly by FPT Metal. Camera
comparison tools normalize native NAADF captures to the same convention while
keeping the unmodified raw captures in their report directories.

For Mandelbulber scenes, the normal topology policy matches its mesh exporter:
march the raw distance field at `0.5 * max_grid_step / detail_level`, or at the
configured constant `DE_thresh`. If the generated FPT evaluator never crosses
that isovalue, the exporter reports `distance-floor-fallback` and the exact
offset used. Such output validates the V7 transport and format but is not a
claim of authoritative Mandelbulber structural parity.

The differential harness is `scripts/run_fptvox7_parity.py`. At output 96 and
mesh 192, three of four reproducible reference scenes have exact occupied-cell
sets; Christmas Ornaments has `0.963922` IoU. Common-cell PBR and emission bits
are exact. Colour is near-exact on Menger FabsAddConditional4D (97.22% exact,
0.011 RGB MAE on a 0..255 scale), but the current generated hybrid colouring
orbit remains materially wrong on Christmas Ornaments.

`scripts/run_fptvox7_exact50_streaming.py` now records
`reference_backend=mandelbulber-cpu-double` and rejects a reference export if
its JSON reports `opencl: true`. This guard was added after auditing the legacy
ranked-50 high-resolution report: 49 of 50 PLY references had been generated by
Mandelbulber OpenCL, whose marching-cubes topology can differ from the CPU
double-precision exporter. Those historical images and metrics are retained as
OpenCL comparisons, not as the structural authority. A replacement CPU/double
cell-only sweep at output 48 / topology 96 completed 49 of 50 scenes, with
mean/median cell IoU `0.9488 / 0.9785` and 35 of 49 scenes at or above `0.95`.
The coarse sweep is a triage gate; high-resolution visual parity remains the
release criterion.

`scripts/run_fptvox7_interop.py` performs the cheaper producer/consumer gate
without generating PLY. The current ranked 50-scene run at output 48 and
topology 96 completes all 50 native Metal dispatches. The former failure,
`asurfKlein_difsGreek`, now exports eight occupied cells and renders through
NAADF. That low-resolution result validates transport and loading, but its
small surface should not be interpreted as a fine-detail structural-parity
claim.

Exact triangle intersection is currently the performance cost center. Fixed
groups of eight triangles with runtime AABBs preserved byte-exact captures but
were rejected because their larger shader footprint regressed low-complexity
cells. The serialized V7 format deliberately contains no group metadata until
a replacement-style acceleration is demonstrated across the required scenes.

### Indexed authored-view triangles (FPTVOX8, experimental)

`--surface-view-indexed-triangles` replaces V7's repeated cell-clipped triangle
fans with one globally quantized source-triangle stream and a per-cell reference
index. It currently supports the authored camera only and implies
`--surface-view-triangles`. V7 remains the default representation.

The 112-byte little-endian header is:

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 8 | `FPTVOX8\0` |
| 8 | 4 | header size, exactly 112 |
| 12 | 4 | version, exactly 8 |
| 16 | 12 | output resolution X/Y/Z |
| 28 | 4 | coordinate system |
| 32 | 24 | bounds min/max as six `f32` values |
| 56 | 8 | occupied cell count |
| 64 | 12 | topology sampling resolution X/Y/Z |
| 76 | 4 | cell record size, exactly 32 |
| 80 | 8 | source-triangle count |
| 88 | 4 | triangle record size, exactly 20 |
| 92 | 4 | reserved, zero |
| 96 | 8 | triangle-reference count |
| 104 | 4 | reference record size, exactly 4 |
| 108 | 4 | reserved, zero |

Each sorted 32-byte cell record stores XYZ, the existing 12-byte packed
material tuple, a `u32` first-reference index, and a `u32` reference count.
It is followed by the source-triangle stream and then the reference stream.
Each 20-byte source triangle contains nine global UNORM16 XYZ components,
relative to the volume bounds, followed by a reserved zero `u16`. Each
reference is a little-endian `u32` source-triangle index.

The producer builds the cell index from the quantized triangle geometry and
uses the same cell-intersection threshold as V7. The NAADF consumer uploads
dense first-reference, reference-count, and material planes followed by the
reference and triangle streams. A dedicated Metal function constant removes
the indexed traversal path from non-V8 kernels.

Both producer and consumer enforce a maximum of 1,024 references in any one
cell. This is a workload-safety contract, not just an allocation limit: an
unbounded fan-out can cause a single ray to perform tens of thousands of
triangle tests and can exceed Metal's watchdog budget. Unsupported surfaces
must be regenerated with the V7 `--surface-view-triangles` layout.

The initial four-scene canary produced these results at the established NAADF
capture settings:

| Rank | V8 refs | V7 clipped triangles | Reduction | Max refs/cell | Paired GPU result | Decision |
| ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 12 | 87,081 | 126,981 | 31.4% | 106 | -16.7% | eligible |
| 8 | 199,922 | 268,701 | 25.6% | 2,585 | +9.9% (mixed) | reject |
| 27 | 266,529 | 400,179 | 33.4% | 566 | -23.2% | eligible |
| 40 | 103,228 | 126,865 | 18.6% | 59,608 | watchdog/timestamp failure | reject |

Rank 12 also improved authored-reference mask IoU from `0.787382` for V7 to
`0.788451` for V8. This small intentional geometry difference comes from
intersecting the globally quantized source triangle rather than a cell-local
quantized clipped fan. It is not byte-equivalent to V7, so structural quality
continues to be gated against the authoritative capture.

The subsequent ranked-50 census classified 25 scenes as fan-out-safe and 25 as
unsafe, with no unexpected export or native-render failures. Across the safe
cohort, median intersection-candidate reduction was `30.65%`, median payload
reduction was `11.25%`, and median V7/V8 mask IoU was `0.98894`. Median
authored-capture IoU changed from `0.91763` for V7 to `0.91774` for V8. The
final selected cohort's per-scene change ranged from
`-0.00157` to `+0.00107` IoU and remained a fine-edge quantization difference,
not a missing-region failure.

`--surface-view-indexed-triangles-auto` is the conservative selector. It emits
V8 only when:

1. maximum references in every cell are at most 1,024; and
2. `100 * (1 - V8 references / V7 clipped triangles)` is at least 28%.

Otherwise it emits ordinary V7 from the same captured triangle stream. The
JSON report records `indexed_selector.selected_layout`, the reason, thresholds,
fan-out, and measured reduction. V7 remains the default when neither indexed
flag is supplied.

The 28% threshold was chosen from alternating timing rather than format size
alone. Every one of the 17 scenes selected from the ranked-50 cohort was timed
and improved: paired medians ranged from `-9.1%` to `-40.8%`, with a cohort
median of `-20.4%`. The boundary experiments also explain the conservative
false negatives: rank 6 won below the threshold, rank 26 was mixed/neutral, and
rank 41 regressed by `6.5%`. Fan-out safety alone is therefore insufficient.
All 50 auto exports were byte-identical to the explicit V8 artifact when
selected or the explicit V7 artifact when rejected.

### Authored-view triangle reconstruction

`--surface-view-triangles` reconstructs a bounded FPTVOX7 surface from one
continuous Metal depth/normal/material capture:

```sh
fpt-metal voxel-export scene.fract --out scene.fptvox \
  --voxel-resolution 192 --surface-view-triangles \
  --surface-view-splats \
  --surface-triangle-resolution 300 \
  --surface-triangle-auto-bounds \
  --mandelbulber-root "$MANDELBULBER_ROOT"
```

Adjacent hit samples form triangles only when their world-space edges fit both
the pixel footprint and output-cell locality limits and their normals remain
coherent. The triangles then use the ordinary in-memory V7 clipping and
quantization path; there is no PLY or GLB intermediary. The standard
`--surface-triangle-threshold-scale` multiplies the default `2x` pixel-footprint
discontinuity limit for this mode. `--surface-triangle-resolution` is the
maximum capture axis; the exporter preserves the authored image aspect on the
other axis and uses the actual capture height for projected pixel footprints.

This representation includes only geometry visible from the authored camera.
The JSON report therefore sets `view_dependent: true`; consumers must not treat
it as a closed or globally complete fractal surface. It is retained as an
explicit structural diagnostic and camera-matched asset path while multi-view
surface fusion remains experimental.

With `--surface-triangle-auto-bounds`, the exporter unions the requested bounds
with the finite visible-hit bounds plus the configured margin. It does not
shrink a caller-supplied volume, and it derives an aspect-matched output grid
to preserve isotropic voxel size. Without the flag, out-of-bounds hits are
discarded and the report still exposes visible versus requested bounds so a
clipped camera-matched export is diagnosable.

`--surface-view-splats` emits two camera-facing triangles for an isolated valid
hit sample or a sample adjoining a rejected connected edge. `FPTNRM1` retains
the sampled source normal for shading. Connected triangles selectively retain
a source-average normal when it differs materially from their final geometric
normal. The half-width is derived from sample
depth and authored camera FOV. It defaults to `0.85x` one capture pixel's world
footprint and a `0.45x` output-cell cap. Disconnected, connected-dominant,
rejected-boundary, and grazing captures use at least a `0.75x` cap; splat-
dominant views retain the compact cap. Callers may tune the explicit cap from
`0.1` to `4.0`. Export reports connected hit pixels, splatted hit pixels, and
splat triangle count separately.

Camera-matched artifacts should use `--surface-triangle-resolution` equal to
the intended render's maximum axis. The authored aspect derives the second
axis. The final first-five gate reaches mask IoU
`0.99027 / 0.99394 / 0.99998 / 1.00000 / 0.99346`. The splat-dominated scene
3 uses a `12.54 MiB` artifact and measured about `0.83 ms` for one-bounce
NAADF. Expanding a mismatched grid with an explicit `2.0` cell cap previously
used `29.25 MB` and `1.217 ms`, so broad splats remain an explicit diagnostic
fallback rather than the production default.

`--surface-view-triangle-dilation 0..1` conservatively expands only connected
triangles touching a rejected depth/normal discontinuity. Expansion stays in
the triangle plane around its centroid, so it preserves the represented local
plane while moving authored sample rays away from fragile boundary edges. The
default is zero. On the iridescence `384`/`300x300` BVH pilot, `0.75` improved
coverage from `95.079%` to `99.630%` with no lost baseline hits and a `6.6%`
increase in clipped triangles. The export report includes `triangle_dilation`
and `surface.dilated_triangles`.

Low-normal-agreement neighbors remain rejected as connected triangles. Their
vertices are tagged and only their existing bounded fallback splats are
expanded from the default `0.85x` footprint / `0.45x` cell cap to at least
`1.0x` / `0.49x`. This preserves both geometric edge limits and avoids adding
the triangle workload of the rejected additive prototype. The ranked-50 gate
improved 47 of 49 renderable scenes, left two unchanged, and moved median IoU
from `0.8890` to `0.9002`; median FPTVOX payload growth was `0.26%`. Export
reports record affected samples as `expanded_low_normal_splats`.

The exporter also recognizes a dense authored capture when at least `99.9%` of
its pixels contain finite in-bounds hits. Only for that class, isolated splats
use at least a `1.5x` projected footprint and `0.49x` cell cap. This is a
replacement footprint policy, not an added triangle class, and avoids applying
full-screen micro-surface reconstruction rules to isolated objects. It selected
22 of 50 ranked scenes: every selected scene improved, the remaining 28 were
byte/metric unchanged, and median IoU moved from `0.9007` to `0.9221`. Median
selected-cohort payload growth was `5.20%`; alternating ranks 8/27/40 timing
canaries measured paired GPU changes of `-5.9%/-5.8%/-12.1%`. Reports count
affected samples as `expanded_dense_view_splats`.

The NAADF comparison harness must preserve explicit pole cameras rather than
clamping them to the interactive mouse-look range. With exact `-90` degree
pitch and pole yaw folded into ray roll, a 50-scene rerun changed only ranks 20
and 36. Their authored-view mask IoU increased from `0.9283` to `0.9361` and
from `0.7690` to `0.8266`, respectively; all other masks and all FPTVOX bytes
were unchanged. A sparse low-normal topology replacement was rejected despite
raising rank 12 IoU from `0.7874` to `0.8490`, because its seven-pair exact
surface benchmark regressed by `7.48%`. A `96^3` packing-grid sweep was also
rejected as a default: median payload fell `27.7%` and median IoU rose from
`0.9221` to `0.9243`, but ranks 42 and 40 lost `0.130` and `0.089` IoU. The
accepted authored-view contract therefore retains the requested `192^3` grid.

A full ranked-50 authored-view census succeeded on 49 scenes. Median mask IoU
improved from `0.7296` for connected triangles to `0.8890` with bounded splats;
the median improvement was `0.1261`, median extra coverage was `0.0364%`, and
there were no IoU regressions. Rank 48 initially failed because its unbounded
capture contained no finite structural hits. Four scenes exceeded `2%` extra
coverage. A targeted
outlier sweep showed that reducing the cell cap to `0.30` or footprint scale to
`0.65` lowered extra coverage but also lowered IoU for every outlier, so neither
setting replaced the `0.85 / 0.45` defaults.

`--surface-view-fit-bounds` is an explicit force mode that quantizes only the
finite captured surface inside the requested domain. A full forced-fit sweep
reduced median authored-view IoU from `0.889` to `0.784`, so it is not a cohort
default. `--surface-view-auto-fit-bounds` is the retained guarded form. It
selects fitted bounds only when the largest captured extent is at most `1%` of
the largest requested extent, and otherwise preserves the expand-only policy
from `--surface-triangle-auto-bounds`. In the ranked-50 gate it selected only
rank 46, improved IoU from `0.0065` to `0.8635`, and left all other 48
successful scene metrics exactly unchanged.

An empty unbounded authored capture triggers one separate finite-domain retry.
The retry begins at the requested voxel AABB, uses at least a `1024x1024`
structural capture, and permits edge-bounded low-normal-agreement triangles
additively without changing the ordinary-scene triangulator. This recovered
rank 48 (`RoadToExascale`) at the requested 300px NAADF view with `0.9878`
mask IoU, `0.91%` miss, and `0.32%` extra coverage. Ordinary canaries 12, 24,
and 44 remained byte-identical. The previous continuous beauty render was not
a valid rank-48 oracle: the legacy path kernel shaded a stalled march position,
while the explicit diagnostic correctly reports no converged unbounded hit.

The corresponding alternating 60-frame timing canaries produced paired GPU
deltas of `+3.24%` on rank 24, `-6.88%` on rank 26, and `-17.90%` on rank 34.
The benchmark harness rejects non-finite, non-positive, and implausibly large
Metal timestamp samples before forming paired deltas. The mixed timing result
is why splats remain an explicit structural option rather than an automatic
performance path.

The independent external gate uses freshly regenerated Mandelbulber CPU/double
PLY surfaces, encoded as exact FPTVOX7 and rendered by the same NAADF visibility
path. Reference generation used `48^3` output and mesh 96, except rank 26 which
required mesh 192. The direct lattice/reference structural scores were near
`0.99` for the cohort, validating the coarse references for silhouette
classification.

Bounds must be identical for this gate. Comparing fixed Mandel references to
auto-expanded view surfaces produced `13.88%` median extra coverage and was
rejected as a domain mismatch. The fixed-bounds gate completed 47 scenes:
median external IoU improved from `0.5997` to `0.7355`, median extra coverage
was `1.3149%`, and source-capture IoU was `0.8387`. Ranks 11 and 40 contained
no reconstructable captured surface inside the fixed domain; rank 48 contained
no unbounded authored-camera hits at that checkpoint. Ranks 17, 30, 32, 36,
and 38 lost external IoU with
splats. These results keep bounded splats explicit and identify formula/camera/
field parity, rather than surface tessellation, as the dominant issue for the
lowest-scoring Mandel outliers. A higher-resolution rank-24 check (`192^3`
output, mesh 384) measured only `0.0036` external IoU, but that result does not
identify false continuous hits: the authored Mandelbulber image and continuous
FPT render both show the large recursive spherical forms, while the CPU/double
PLY and topology lattice are sparse. The CPU mesh exporter is not a structural
oracle for this formula, so rank 24 remains gated against the authored
continuous capture instead.

Use `--surface-view-capture-cache capture.bin` to reuse the expensive
continuous structural capture across connected/splat parameter experiments.
The adjacent `capture.bin.json` records contract version, record size,
requested and effective resolution, bounded-fallback selection, generated-source
SHA-256, exact camera/FOV bit patterns, export-bound bit patterns, and world
scale. Missing cache pairs are created together; partial or mismatched pairs
fail closed. The cached binary remains the same
64-byte-per-pixel structural stream consumed by the direct Rust triangulator.

`--surface-view-auxiliary-views 4|6|12` captures local parallax views and
triangulates every image independently. The merger gives primary authored-view
cells precedence and admits only previously absent auxiliary cells; pixels
from different cameras are never connected. This is an offline completeness
experiment. It can increase runtime traversal work and currently remains
disabled by default.

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
