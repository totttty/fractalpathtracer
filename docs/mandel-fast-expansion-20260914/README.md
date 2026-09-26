# Fast Remaining-Scene Expansion

[Reviewed showcase](../mandel-showcase/README.md) | [Catalogue](../mandel-catalog/README.md) | [Evidence](evidence.json)

Fresh low-cost execution tests cover all **116 scenes without completed visual reviews**. **88** complete both FPT modes; **64** have complete matching-resolution native/FPT triplets.

These are screening previews. The 423-scene reviewed gallery and previous decisions remain unchanged. The 207 existing visual holds, including user-deferred 095, are not retried. No scene-specific fixes, renderer edits, or source-scene edits.

All captures retain authored aspect ratio at 150px maximum edge. FPT uses 4 SPP with chunked accumulation; native CPU sampling caps MC at one only where already enabled. Native effects remain authored; this is not equivalent integrator sampling. Neutral FPT is a structure aid, not a matched native neutral-lighting reference.

Each FPT mode is bounded to 15 seconds and each requested native reference to 10 seconds. CPU/GPU work overlaps within a scene. Failed or timed-out attempts are shown, with no automatic retry. Unrequested references retain their prior incomplete status. Execution success does not certify visual fidelity.

Capture suites took 9.7 minutes including per-suite overhead; this is not GPU benchmarking.

[Ten candidates for full-quality review](shortlist.md) are selected from visual inspection of the previews. [Recorded capture commands](capture-commands.json) preserve the run settings.

## Comparisons

- [Page 1](page-01.md): 071, 088, 097, 102, 134, 172, 182, 186
- [Page 2](page-02.md): 295, 298, 307, 364, 396, 397, 404, 405
- [Page 3](page-03.md): 407, 408, 416, 417, 419, 425, 426, 432
- [Page 4](page-04.md): 436, 437, 439, 441, 442, 443, 447, 449
- [Page 5](page-05.md): 450, 454, 455, 456, 457, 458, 461, 463
- [Page 6](page-06.md): 464, 465, 466, 467, 469, 470, 471, 473
- [Page 7](page-07.md): 475, 477, 480, 481, 482, 484, 485, 489
- [Page 8](page-08.md): 490, 491, 495, 501, 502, 503, 505, 506
- [Page 9](page-09.md): 513, 514, 515, 519, 521, 523, 524, 525
- [Page 10](page-10.md): 526, 530, 537, 538, 539, 540, 541, 543
- [Page 11](page-11.md): 549, 550, 553, 554, 555, 558, 561, 562
- [Page 12](page-12.md): 576, 589, 620, 641, 646, 653, 654, 659
- [Page 13](page-13.md): 662, 674, 678, 680, 694, 695, 697, 698
- [Page 14](page-14.md): 699, 701, 705, 710, 712, 720, 729, 731
- [Page 15](page-15.md): 739, 741, 743, 744
