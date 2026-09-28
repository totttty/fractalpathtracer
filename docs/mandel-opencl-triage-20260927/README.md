# OpenCL-Assisted Triage — 27 September 2026

[Reviewed gallery](../mandel-showcase/README.md) | [Triage decisions](triage-decisions.json) | [Gallery assessment](assessment.json) | [Batch provenance](batch-provenance.json) | [Native renderer audit](../mandel-native-renderer-audit-20260926/README.md)

This pass triaged all 89 experimental scenes, then promoted the strongest candidates with full CPU references. The gallery now contains **436 accepted scenes** (9 new). Scene 561 is held, and **79 experimental scenes** remain.

## Method

1. **Native previews.** Every experimental scene was rendered natively with Mandelbulber's OpenCL renderer, using the local robustness build (`fpt/opencl-robustness`, `c498128f6`). That build re-renders tiles the GPU abandoned and fails loudly instead of saving corrupt images. The previews used gallery framing, 300px and authored sampling, with a 300 s limit. OpenCL previews **are not references**. MC global illumination and iteration fog differ from CPU (see the audit).
2. **FPT captures.** FPT neutral and authored captures were made at gallery settings, using `fpt-metal` built from `6359aa1`. That build reproduces historical captures byte-for-byte.
3. **Triage.** Each native / neutral / authored triplet was reviewed by eye and sorted:

   | Triage group | Scenes |
   | --- | ---: |
   | Strong candidates | 17 |
   | Possible, with colour/fog limitations | 18 |
   | Likely hold | 34 |
   | Native OpenCL unusable (timeout/exit 3), needs a CPU reference | 8 |
   | FPT GPU-watchdog failure | 10 |
   | FPT rejects the scene | 2 |

4. **Gallery evidence.** Ten strong candidates received native CPU references with the original binary, authored sampling and no reduced sampling. They were then reviewed as ordinary gallery triplets at 300px and 32 SPP.

## Gallery result

| Decision | Scenes |
| --- | --- |
| accepted | 678, 469 |
| accepted-with-limitations | 182, 514, 680, 489, 477, 443, 485 |
| needs-work | 561 (blue-grey sky and haze become orange-brown) |

The evidence epoch is `opencl-triage-2026-09-27`. Native CPU references and FPT captures came from two suite runs with identical manifest, harness hashes, FPT binary and settings, merged without re-rendering. See [batch provenance](batch-provenance.json).

The raw captures from the previous batches are gone, so `publish_mandel_review.py publish --previous docs/mandel-showcase` reused the committed showcase images for the 637 existing scenes. Each image was checked against the showcase manifest hash. All existing scene pages and images are byte-identical.

## Remaining queue

- **7 strong candidates without CPU references:** 549, 407, 550, 450, 540, 674 and 432. Scenes 674 and 432 previously exceeded 420 s and 600 s of CPU time.
- **18 possible scenes** with colour or fog limitations.
- **8 scenes whose OpenCL native preview failed.** FPT output looks plausible, so a CPU reference is needed to judge them.
- **10 scenes where macOS's GPU watchdog kills FPT's long command buffers** ("Impacting Interactivity"), even with exclusive GPU use: 172, 186, 295, 298, 307, 442, 464, 495, 641 and 646. FPT reports these as errors, not bad images.

## Caveats

- During this pass the native-reference cache hashed the whole process environment, so app or session restarts invalidated every entry. Contract version 2 now hashes only renderer-relevant variables (`HOME`, `LANG`, `TZ`, and the `LC_`, `QT_`, `OMP_`, `KMP_`, `DYLD_`, `MANDELBULBER`, `OCL_` and `OPENCL_` prefixes).
- Timings were measured under unrelated machine load and are not benchmarks.
