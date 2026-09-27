# Native Mandelbulber Renderer Audit — 26 September 2026

[Full catalogue](../mandel-catalog/README.md) | [Evidence](evidence.json)

This audit examined the native Mandelbulber references used for FPT visual review. No acceptance decisions change here. It records four facts that affect how existing and future native references should be read, plus an OpenCL evaluation and a set of local Mandelbulber fixes.

Pinned upstream revision: `230456cee40968cbaa7f301bba91daa4865a29db`. Machine: Apple M1 Max. Unrelated jobs kept the load average between 40 and 220 throughout, so no wall time here is a benchmark.

## 1. The CPU renderer is not deterministic

Three runs of the original binary on the same scene differ from each other:

| Scene | Run-to-run MAE / 255 | Pixels changed | Committed reference vs fresh run |
| --- | ---: | ---: | --- |
| 05 | 0.03 | ~3,700 (5.5%) | within run-to-run range |
| 596 | 0.64 | ~21,000 | within run-to-run range |
| 50 (DOF blur) | 7.35 | ~62,000 | within run-to-run range |

Committed references are reproducible only statistically, never byte-for-byte. Any "same output" test for a native binary has to compare against this spread.

## 2. CPU Monte Carlo global illumination double-counts surface bounces

`src/shader_global_illumination.cpp` adds each bounced surface's radiance twice: unclamped at lines 159-161, and clamped again at lines 177-183. The OpenCL kernel (`opencl/engines/shader_global_illumination.cl:232-234`) adds it once. As a result, CPU references for scenes with `DOF_monte_carlo` and `DOF_MC_global_illumination` are brighter than the renderer's own OpenCL path. They also partly ignore `MC_GI_radiance_limit`.

Affected catalogue scenes: 7 reviewed (21, 49, 398, 420, 484, 627, 640), 5 blocked (386, 394, 411, 696, 699), and 36 experimental. **Decision deferred:** existing reviews stand. These scenes were compared against over-bright references.

## 3. Chromatic aberration exists only in the OpenCL renderer

`post_chromatic_aberration_*` is applied only by the OpenCL post-filter path (`src/render_job.cpp`). CPU references silently omit it. Scene 50 (*hex prism and chromatic*) is the only affected catalogue scene. Its accepted CPU reference is sharp, while the authored look has heavy radial blur and colour fringing.

## 4. OpenCL native rendering: fast, but unsafe without fixes

OpenCL renders the gallery settings in about 1-85 s, compared with minutes on CPU. Against the committed CPU references for 52 accepted scenes, all 16 accepted MC scenes included:

| Outcome | Scenes |
| --- | --- |
| Near-identical (under 1/255) | ~20 non-MC scenes |
| Same image, MC grain or sub-pixel aliasing (3-20/255) | most MC scenes; 160, 412 |
| Darker global illumination (see 2) | 21, 398, 420; milder 484, 640 |
| Chromatic aberration present (see 3) | 50 |
| Missing iteration-fog haze | 701 |
| Black image, exit 0 | 627 |
| Garbled or duplicated tiles, exit 0 | 25, 701 |

The unpatched OpenCL path is unsafe because of how Apple's driver behaves:
- It abandons long dispatches, while `clFinish` and the kernel event both still report success.
- `enqueueFillBuffer` leaves the output buffer unchanged.
- Headless mode ignores render failure, saves the image and exits 0.

`opencl_job_size_multiplier=1` hides some cases, such as scene 25, but not scene 701.

## Local Mandelbulber fixes

Branch `fpt/opencl-robustness`, commit `c498128f6`, in `/Volumes/Ventura/Projects/mandelbulber2-fixes`. It is local only and not pushed.

- **Tile verification.** The output buffer is marked with an explicit blocking write, and the write is read back to confirm it landed. Pixels the GPU abandoned are re-rendered in halving sub-dispatches down to 32 px. Any remaining gap fails the render.
- **Loud failure.** A failed OpenCL fractal pass can no longer be overwritten by the SSAO, DOF or post-filter results. Headless still renders exit with code 3 and save no image.
- **CPU chromatic aberration.** A float port of `chromatic_aberration.cl`, applied after HDR blur and per stereo region. Alpha is left unchanged.

Verification:

| Scene | Result |
| --- | --- |
| 25 | 4 runs identical; 0.17/255 vs CPU; retries recovered 13,088-14,048 abandoned pixels in 2 runs |
| 701 | complete and deterministic; the remaining difference is iteration fog, not corruption |
| 627 | exit 3, no image (the kernel fails outright; CPU only) |
| 05, 160, 21 | pixel-identical to unpatched OpenCL |
| 50, CPU | vs OpenCL: 25.4 → 8.2/255, which is about the CPU DOF run-to-run spread |
| 05, 596, CPU | within the original binary's run-to-run spread |

## Rebuild with OpenMP and `-mcpu=apple-m1`: not adopted

On the CPU path, OpenMP covers only the non-MC DOF blur and HDR blur. The MC denoiser runs only in OpenCL. Upstream disabled OpenMP for Apple Silicon in passing, not for a correctness reason.

A control rebuild, an OpenMP build and an OpenMP + `-mcpu` build all fall inside the original binary's run-to-run spread on scenes 05, 596 and 50. Interleaved timing showed no useful speedup; at most about 10% less CPU time on scene 50, at 1 s timing resolution. Only 2 of the 89 queued scenes (489, 646) use the OpenMP paths. The references keep the original binary.

## Also recorded

- The ignored `reports/` tree holding raw captures and the native reference cache is no longer present, so completed captures cannot be reused. Committed showcase detail PNGs retain the native reference pixels at captured size.
- Iteration fog (28 catalogue scenes) differs between the OpenCL and CPU renderers. This is not investigated yet.
