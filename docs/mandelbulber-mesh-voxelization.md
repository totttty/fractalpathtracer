# Mandelbulber Mesh Voxelization

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
  --mandel-reference-out scene.mandelbulber.png \
  --mandel-reference-size 900x600 \
  --mandel-mesh-opencl
```

Use `--mandel-mesh-resolution N` when the marching-cubes sampling resolution should differ
from the output voxel resolution. It defaults to `--voxel-resolution`. Thin distance fields
can require a finer sampling grid even when the final voxel resolution is unchanged. For
example, `asurfKlein_difsGreek` produced only eight triangles over `[-4,4]^3` at 192 samples,
but 2,073,182 triangles at 384 samples; the latter conservatively reduced to 237,747 occupied
192^3 cells.

`--mandel-mesh-ply-out` preserves the exact raw PLY emitted by Mandelbulber instead of keeping
it only in temporary storage. `--mandel-reference-out` renders the original `.fract` through
the same external Mandelbulber binary and records its timing and dimensions in the export
report. `--mandel-reference-size` defaults to the scene's own image dimensions.

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
