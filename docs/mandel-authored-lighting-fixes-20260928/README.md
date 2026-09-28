# Authored Lighting Fixes — 28 September 2026

[OpenCL-assisted triage](../mandel-opencl-triage-20260927/README.md) | [Native renderer audit](../mandel-native-renderer-audit-20260926/README.md)

Branch `fpt/authored-lighting-fixes` fixes general gaps in FPT's authored Mandelbulber path. The OpenCL triage found 14 authored renders that came out almost entirely black or blown out to white. Diagnosis split them into 7 causes; each cause was confirmed by one-parameter re-renders. This branch fixes five of them. It does not change any gallery decision: scenes whose renders change need fresh captures and a fresh review before promotion.

## Fixes

| Commit | Fix | Mandelbulber behaviour matched |
| --- | --- | --- |
| `db3c518` | Emission is RGB: `luminosity × luminosity_color`, or `luminosity × gradient(palette position)` when the luminosity gradient is enabled. It is added directly rather than multiplied by the surface colour. Primitives that take their colour from the palette no longer inherit gradient-driven fractal emission. | `shader_object.cpp:150-167`, `shader_surface_color.cpp:43` |
| `3bf21cb` | `hdr` applies `tanh` after the contrast clamp. | `cimage.cpp:317-360` |
| `1f18f25` | A point-type `light1` is a point light at `light1_position`, with the directional sun off. Spot, projection and beam `light1` are reported as unsupported. | light1 defaults in `initparameters.cpp` |
| `20d74f9` | Surface transparency blends the surface, reflected and transmitted parts. `transparency × (1 − reflectance)` goes to the refracted ray, tinted by the transparency colour. Surface lighting stays at full strength, weighted by the surface share. | `render_worker.cpp:1269-1291` |
| `b9d29b7` | Volumetric, basic and iteration fog, clouds, and visible fake-light glow are listed in `mandel_auxiliary.unsupported`. `render.json` records the effective lighting setup in `mandel_lighting`. Pixels are unchanged. | — |

## Verification

Renders are authored at 300px and 32 SPP.

| Scene | Cause | Before (mean luma / % clipped) | After | Native |
| --- | --- | --- | --- | --- |
| 467 | luminosity gradient ignored | 255 / 100% | 52 / 0.1% | no reference |
| 466 | luminosity gradient ignored | 230 / 85% | 147 / 2.8% | 150 / 0.9% |
| 697 | luminosity gradient ignored | 198 / 77% | 129 / 6.4% | 76 / 0.5% |
| 695 | luminosity colour ignored | 0 (black) | 91 / 0.2% | no reference |
| 539 | `hdr` missing | 172 / 44.5% | 141 / 2.5% | 137 / 0% |
| 408 | point light1 treated as directional | 0.6 | 51 | 53 |
| 741 | transparency removed surface light | 0.0 | 46 | 88 (fog and light glow not rendered) |

Scenes 425, 426, 543 and 417 remain dark. Their light comes from fog or fake-light glow, which the path renderer does not implement, and they are now flagged as unsupported instead of failing silently.

**Regression set:** 469, 678, 182, 05, 064, 151, 304, 319, 540, 549, 485, 477 and 514. Every commit keeps these pixel-identical except where a scene uses the changed feature:
- **`hdr`:** 151 moves closer to native (MAE 13.3 → 11.0/255). 182 moves further away (28.2 → 34.1): it was already underexposed, and `tanh` darkens it more.
- **Point light1:** 429 changes slightly (MAE 0.176 → 0.173 on a 0-1 scale) but is now brighter than native.
- **Transparency:** 549 has a transparent water plane and is essentially unchanged. Among accepted transparent scenes, 542, 740 and 745 improve. 638 (palette transparency gradient) and 739 (subsurface scattering) get slightly worse, because FPT supports neither feature. The transparency fix is kept as a deliberate trade-off.

The full `cargo test --release --locked` passes: 174 library, 62 GPU binary, 24 fptvox and 3 voxel-library tests.

Contact sheets: [emission and hdr targets](emission-hdr-targets.jpg), [light1 and transparency targets](light-transparency-targets.jpg).

## Not fixed

- **Secondary sky light without global illumination** (436, part of 539). The authored path adds `background_color_2 × 0.45` on every escaped bounce, while Mandelbulber adds sky light only with Monte Carlo global illumination. Gating the term fixed 436 (MAE 89.9 → 43.4) but darkened 6 of 7 ordinary accepted scenes: in FPT the term stands in for Mandelbulber's ambient fill. **Decision: leave it; 436 stays held.** The proper fix is a Mandelbulber-style ambient term for scenes without global illumination; the gate itself is kept as [parked-sky-light-gate.patch](parked-sky-light-gate.patch).
- **Fog and fake-light glow** in the path renderer (425, 426, 543, 417). Deferred with the other fog and cloud work.
- **Smaller gaps:**
  - 697 is still brighter than native, because the emissive cube lights the floor strongly.
  - 466's cylinder lamp is missing, since cylinder primitives are unsupported.
  - 408 renders green where native is blue, which looks like a palette issue.
  - `luminosity_emissive` is parsed but not applied.
  - Mandelbulber ignores transparency when `raytraced_reflections` is off; FPT does not.
  - FPTVOX exports a point-type light1 as a directional light.

## Before any gallery change

These commits change the authored output of some accepted scenes, at least 182, 151 and 429, plus the transparent scenes above. Previous acceptances stay tied to their original captures. A gallery refresh with this renderer needs new captures and explicit re-review of the changed scenes. Held scenes that now render sensibly, such as 466 and 539, need fresh CPU references before they can be reviewed.
