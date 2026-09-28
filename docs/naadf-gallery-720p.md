# NAADF 720p gallery - 15 September 2026

[Open the private gallery](https://naadf-scenes-720p-20260915.totttty.chatgpt.site).

All 427 currently reviewed FPT catalogue scenes were rendered with the installed adaptive NAADF renderer at **1280×720, 32 samples and 4 bounces**. This is the same set as the prior 427-scene adaptive transmission execution screen.

The full batch completed in 427.4 seconds with two workers. All 427 captures passed independent checks of source manifest and image hashes, renderer identity, actual render statistics, image dimensions, and non-uniform/non-black output. The renderer was unchanged during the run. The gallery has one full-resolution image and one 480×270 thumbnail for every scene, in numeric order.

- Local gallery: `output/naadf-gallery-720p-20260915/site/dist/index.html`
- Capture report: `output/naadf-gallery-720p-20260915/report.json`
- Independent verification: `output/naadf-gallery-720p-20260915/verification.json`
- Original lossless output: `output/naadf-gallery-720p-20260915/captures/<id>/render-naadf_aadf_cpu-normal.ppm`
- Per-scene provenance: `output/naadf-gallery-720p-20260915/captures/<id>/receipt.json`

The gallery uses WebP quality 75 for full-resolution viewing and quality 84 for lazy-loaded thumbnails. Original PPM images remain available locally. Render dimensions are fixed at 16:9; scene cameras and adaptive geometry come from the validated bundles.

This run raises output resolution and sample count; it does not rebake adaptive geometry or fix the documented visual-parity gaps. No NAADF visual acceptance was promoted.

## Reproduce or resume

```sh
/opt/homebrew/bin/python3.11 scripts/naadf/render_naadf_gallery.py \
  --native /Users/jordantotty/Desktop/vox/metal-voxel-fractal-integration \
  --catalog-report /Users/jordantotty/Desktop/vox/metal-voxel-fractal-integration/reports/fpt-transmission-catalog-20260914/report.json \
  --output output/naadf-gallery-720p-20260915 --jobs 2
/opt/homebrew/bin/python3.11 scripts/naadf/verify_naadf_gallery.py \
  output/naadf-gallery-720p-20260915
```

The runner checks the current renderer against the source report, validates bundle hashes, resumes matching captures, and retries incomplete scenes. The separate verifier checks every gallery link, full-size image, thumbnail and capture, without relying solely on the runner's success count.
