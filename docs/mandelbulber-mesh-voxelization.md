# Mandelbulber Mesh Voxelization

## Correct Mandelbulber marching-cubes reference

Mandelbulber 2 currently skips edge extraction for its first marching-cubes
slab. The following slab can then reuse uninitialized shared-vertex indices,
producing nonlocal triangles and omitting valid first-slab geometry. Apply the
included source patch before building the binary used for authoritative parity
captures:

```bash
git -C /path/to/mandelbulber2 apply \
  /path/to/FPT-metal/patches/mandelbulber2-marching-cubes-first-slab.patch
```

The FPT PLY importer also discards triangles whose edges exceed one local cube
diagonal (with a small numerical tolerance). This protects ordinary imports
from corrupt output, but it cannot reconstruct the omitted slab; use the
patched reference binary for exact parity measurements. The export report's
`discarded_nonlocal_triangles` field should be zero with a corrected binary.

`fpt-metal voxel-export` has an opt-in experimental route that uses Mandelbulber's own
distance evaluator and marching-cubes mesh exporter before producing native FPTVOX6 surface
cells. It is intended for structural experiments where the default point-sampled Metal
voxelizer loses thin or disconnected surface features.

```bash
fpt-metal voxel-export scene.fract \
  --out scene.fptvox \
  --voxel-resolution 192 \
  --bounds-min '-4,-4,-4' \
  --bounds-max '4,4,4' \
  --mandelbulber-root /path/to/mandelbulber2 \
  --surface-source mandelbulber-mesh \
  --mandelbulber-bin /path/to/mandelbulber2 \
  --mandel-mesh-resolution 384 \
  --mandel-mesh-ply-out scene.mandelbulber.ply \
  --mandel-mesh-auto-bounds \
  --mandel-mesh-auto-bounds-margin 0.10 \
  --mandel-reference-out scene.mandelbulber.png \
  --mandel-reference-size 900x600
```

Use `--mandel-mesh-resolution N` when the marching-cubes sampling resolution should differ
from the output voxel resolution. It defaults to `--voxel-resolution`. Thin distance fields
can require a finer sampling grid even when the final voxel resolution is unchanged. For
example, `asurfKlein_difsGreek` produced only eight triangles over `[-4,4]^3` at 192 samples,
but 2,073,182 triangles at 384 samples; the latter conservatively reduced to 237,747 occupied
192^3 cells.

Mandel mesh exports can preserve cubic world-space cells across highly anisotropic bounds with
`--mandel-mesh-voxel-min-axis-resolution N`. The minimum world-space axis receives `N` cells;
the other logical dimensions grow in proportion to their spans. The FPTVOX1 output remains a
sparse occupied-cell stream, so a grid such as `1039x64x1033` does not allocate its dense
logical capacity. The CLI accepts up to 1024 cells on the minimum axis for this sparse
Mandelbulber mesh path; the general dense Metal voxelizer remains capped at 512. The native
NAADF consumer must use its CPU record builder for grids whose
dense capacity exceeds the GPU builder's resident-material limit.
The same resolution control also applies to bounded-patch and exact-surface Mandel mesh
exports, preventing anisotropic source bounds from silently producing non-cubic cells.

Interior-camera fractals need capture bounds around the camera as well as the visible target.
`--mandel-mesh-camera-safe-bounds` preserves the requested box but expands any face that lies
closer to the authored camera than `--mandel-mesh-camera-margin` times the box's largest span
(default `0.10`). It is deliberately inert when the camera is outside the requested box, so an
ordinary exterior object shot does not acquire a large empty volume between camera and object.
The export report records requested and effective bounds plus the canonical world- and
grid-space camera positions. FPT orientation fields are named explicitly because a consumer's
camera-forward convention can differ. This mode prevents near-camera clipping; it does not
claim to bound an infinite fractal or repair holes already present in the marching-cubes mesh.

`--mandel-mesh-voxel-dilation 1` adds a conservative Euclidean one-cell shell around the
triangle-intersection cells. Radius one adds the six axis neighbours, not all 26 cells of a
3x3x3 cube. This is useful when a fine one-cell triangle shell becomes subpixel and visibly
perforated in voxel mode. It is an explicit geometry-quality tradeoff: files and resident
records grow, while exact-surface exports remain unchanged. Increase the marching-cubes mesh
resolution before enabling dilation; dilating an undersampled source mesh only enlarges its
sampling artifacts.

`--mandel-mesh-ply-out` preserves the exact raw PLY emitted by Mandelbulber instead of keeping
it only in temporary storage. `--mandel-reference-out` renders the original `.fract` through
the same external Mandelbulber binary and records its timing and dimensions in the export
report. `--mandel-reference-size` defaults to the scene's own image dimensions.

The structural parity harness uses Mandelbulber's CPU/double mesh exporter as its authoritative
reference. `--mandel-mesh-opencl` is an explicit performance experiment: it now passes numeric
enum values required by Mandelbulber's command-line decoder, but Mandelbulber's OpenCL slicer can
produce topology that differs materially from its CPU marching-cubes path. Do not use it to
produce the CPU parity baseline.

Image-space parity keeps native NAADF captures in their recorded orientation by default. The
camera conversion already accounts for the renderer yaw convention; applying an additional
horizontal mirror worsens depth agreement. `render_fptvox7_parity_sheets.py` records the chosen
orientation and exposes `--native-capture-orientation mirror-x` only as a diagnostic override.
Mandelbulber-backed FPT cameras use a half-width image-plane convention, while NAADF constructs
rays over the full `[-1,1]` NDC span. The parity script therefore converts the internal FPT FOV
with `2*atan(0.5*tan(fpt_fov/2))` and requests a `(-0.5,-0.5)` pixel offset to reproduce FPT's
corner-sample convention. Visibility caches include this camera contract so captures made with
the earlier direct-FOV mapping cannot be reused silently.

Parity reports use a tessellation-invariant structural score: the minimum of occupied-cell IoU
and per-cell triangle-area overlap. Cell IoU alone is not a surface-fidelity metric. A candidate
can occupy almost exactly the same coarse output cells while omitting internal sheets or changing
sub-cell surface placement. Triangle-count overlap remains a diagnostic column, not a gate:
equivalent surfaces can be subdivided into different numbers of valid triangles.

Some chaotic formulas are precision-sensitive across evaluator backends. In the ranked validation
suite, `Jos Leys Kleinian sphereInversion` reached `0.9741` occupied-cell IoU at a 511-sample mesh
grid, but retained only `84.86%` of the CPU-reference triangles and scored `0.8426` on per-cell
triangle-count overlap. An exact lattice capture showed that this was not an iso-threshold bias:
the disagreeing samples were generally far from the threshold and increasing the sampling rate
did not close the surface gap. Mandelbulber CPU/double PLY extraction remains the authoritative
fallback when exact topology is required for such a scene. Metal fp32 and Mandelbulber OpenCL
outputs must be reported as backend-specific approximations rather than CPU-parity results.

`riemann bulb msltoe mod2 001` remains precision-sensitive. The original Formula85 specialization
used a two-sided minimum derivative heuristic. It produced high occupied-cell IoU but substantial
surface excess: at a 192-sample grid its structural score was `0.7490`, and at 384 it fell to
`0.6426`. Matching Mandelbulber's one-sided Delta-DE stencil raises those scores to `0.8928` and
`0.8254`, respectively. At 192 samples, visible IoU improves from `0.99859` to `0.99927`, mean
normal error falls from `20.20` to `14.36` degrees, and depth MAE falls from `0.4467%` to `0.2959%`.
This is the retained Formula85 path, but it is still an fp32 approximation: at 384 samples it emits
`23.63%` more clipped triangles and `21.00%` more triangle area than the CPU/double reference.
Mandelbulber CPU/double PLY extraction therefore remains authoritative when exact topology is
required. Do not treat either high cell IoU or increased sampling resolution as proof of parity.

Direct V7 mesh extraction uses a dedicated Delta-DE probe for eligible hybrid scenes. Authored
advanced-quality scenes retain their `deltade_relative_delta`; other eligible scenes use a
`0.05` detail-relative probe with a `5e-7 * length(point)` fp32 stability floor. Against the
CPU/double PLY reference, `hybrid005` improved from `0.93379` to `0.93755` cell IoU at 48/96
sampling, from `0.93938` to `0.94251` at 96/192, and from `0.94779` to `0.94995` at 192/384.
A scan of the other 49 ranked scenes produced byte-identical occupied-cell sets. The probe is
isolated to offline mesh extraction and does not change continuous rendering.

`--mandel-mesh-auto-bounds` is an opt-in two-pass policy for thin objects that occupy only a
small fraction of the requested cube. The first authoritative mesh establishes object bounds.
The exporter then builds a scene-centered cube whose side is the mesh's largest dimension plus
the total margin selected by `--mandel-mesh-auto-bounds-margin` (default `0.10`). The candidate
for exact triangles and bounded patches is accepted only when the discovery volume has no
occupied boundary cells, the new cube is strictly inside the original domain, and the candidate
has no occupied cells on any of its six faces. Candidates are also limited to 50%
secondary-patch cells: denser multi-plane surfaces can recover real sub-voxel openings while
becoming less representable as a conservative 192^3 shell, so they retain the discovery volume
instead. Otherwise the original volume and PLY are retained.
Export JSON records the candidate, acceptance state, boundary counts, surface-complexity ratio,
threshold, and fallback reason under `auto_bounds`.
It also reports discovery, candidate, and total pass time so the two-pass export
cost remains explicit.

When combined with `--mandel-mesh-voxel-cells`, auto-bounds may accept a tight candidate whose
occupied cells reach the candidate boundary. This is an explicit view-volume crop used to spend
the requested voxel resolution on the visible object instead of a much larger mostly empty
domain. Its centered cube may extend outside a thin original view volume, and its aspect-aware
voxel resolution is recomputed for that cube. Reports identify a boundary-touching accepted
candidate as `accepted_cropped_voxel_cell_bounds`. The discovery volume must still be free of
occupied boundary cells. Exact triangle and
bounded-patch exports retain the strict six-face boundary rejection because silently clipping
those surface representations would change their geometry contract.

## Pipeline

1. The external Mandelbulber executable evaluates the requested fixed world bounds.
2. Mandelbulber writes its binary little-endian PLY mesh with vertex RGB and triangles.
3. FPT-metal transforms Mandelbulber Z-up coordinates to the public FPTVOX Y-up convention.
4. Triangles are conservatively intersected and clipped against output cells.
5. Up to two dominant clipped surface patches are encoded per occupied cell in FPTVOX6.
6. The native NAADF renderer consumes the file directly; GLB and MagicaVoxel are not used.

The PLY reader deliberately accepts only Mandelbulber's exact current property order. The
exporter's `s` and `t` values are ignored because geometry, RGB, and triangle indices are the
only fields needed by this path.

## Compatibility and limits

- This mode requires `.fptvox` output and cannot be combined with Metal surface sampling,
  local-parallax promotion, interior filling, or `--surface-band`.
- Fixed output bounds are retained. Tight PLY bounds are never renormalized.
- Geometry and vertex RGB are authoritative. PLY does not carry FPT PBR, emission,
  transmission, IOR, or reliable signed normals; the loaded scene supplies scalar material
  defaults and the converter derives patch normals from triangle winding.
- Original Mandelbulber output is the authority for this mode. Differences from FPT's
  generated Metal evaluator are recorded for later compatibility work and do not invalidate
  the Mandelbulber mesh.
- A low triangle count does not imply an unsupported formula. Marching cubes samples a finite
  grid and threshold, so thin or difficult distance fields may require a larger
  `--mandel-mesh-resolution`. Tightening bounds can also reveal detail, but must not be used if
  the extracted surface touches the box boundary because that indicates cropping.
- This is an offline conversion path. Mesh extraction and conservative triangle voxelization
  are not part of the NAADF frame loop.

Mandelbulber remains an external GPL process. No Mandelbulber source or linked GPL code is
included in the Apache-2.0 FPT-metal library.
