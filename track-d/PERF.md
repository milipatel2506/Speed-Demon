# Track D: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 72.69 | 893 | 0 |
| 1 | D1 baseline: Rust, CSR forward and reverse, bidirectional Dijkstra with a binary heap, 8-query batches over threads | | 2.68 (local) | 327 (local) | 34.2 (local estimate) |

## Step 1 notes

The step 1 row is **not a board number**: no board run ID yet. It was measured with the harness's `run_submission.py` gates (correctness, timeout and sandbox all pass) on a 4-CPU VM, not the scoring box, so only the board's number counts. `full`, seed 1, 5 runs, middle three averaged, 4 worker threads (the VM's CPU count); the memory is the cgroup high-water mark, as on the board. The points are the K formula with the S measured by rustfmt defaults (266 code lines) and the reference's S of 43, D = 1.

Phase breakdown inside the container (milliseconds, `full`, seed 1, 4 workers, typical of three runs): read and parse 166, CSR build 255, queries 2,330, write under 1.

With 12 worker threads forced on the same 4 CPUs: T 2.70 s, M 775 MB (each worker keeps two label arrays of 1.74 M entries of 16 bytes). Every one of the 12 datasets (tiny, sample and full, seeds 1 to 4) and the spec's worked example give the reference's SHA-256.
