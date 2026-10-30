# Track A: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 32.05 | 228 | 0 |
| 1 | Baseline: simple single-threaded Rust, `BTreeMap` of (hour, endpoint), std only | pending first board run | 4.4 (local container, not the box) | 270 (local peak RSS) | pending |

Step 0 is the scoring box's calibration of the reference on seed 1. Step 1 was measured locally: the image was built offline and run with the board's limits (read-only root, 1 GB `/tmp`, uid 65534), full dataset, output hash matching the reference. The first run of the binary outside Docker took 8.8 s on a cold page cache. These are not the box's numbers, so replace them with the board run's.
