# Track A: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 32.05 | 228 | 0 |
| 1 | Baseline: simple single-threaded Rust, `BTreeMap` of (hour, endpoint), std only | | 1.90 | 268 | |
| 2 | Opt 1: `mmap` the file via `extern "C"` (no crate), hand-written byte parser (hour = first 13 bytes, digits parsed directly) | | 1.39 | 40 | |
| 3 | Opt 2: `HashMap` keyed by (hour, endpoint bytes) instead of `BTreeMap`; sort only the final group list | | 0.85 | 41 | |
| - | Failed experiment: also cache the last (hour, endpoint) key and its slot so a repeat skips the hash lookup. Same hashes, but slower (0.91 s vs 0.85 s, two A/B runs each) and +9 MB, because consecutive lines rarely share an endpoint. Reverted | | 0.91 | 49 | |
| 4 | Opt 3: one thread per available CPU, file cut after a newline into equal chunks, one table per thread, tables merged at the end | | 0.57 | 55 | |
| - | Failed experiment (opt 4): `select_nth_unstable` for p99, then p95 and p50 in the prefix below it, instead of sorting each group. Same hashes, but no measurable gain (0.54 s vs 0.54 s and 0.49 s vs 0.55 s, A/B back to back): the average group holds about 24 latencies, so the sort is already cheap, and it added about 9 lines of code. Reverted | | 0.52 | 53 | |
| - | Failed experiment (opt 5a): `lto = "fat"`, `codegen-units = 1`, `panic = "abort"` in the release profile (opt-level 3 was already set). Same hashes, no gain: 0.51 s and 0.52 s against 0.48 s and 0.52 s for the same code without them, so I left the profile alone | | 0.52 | 54 | |
| 5 | Opt 5b: `-C target-cpu=native` in `.cargo/config.toml`. Five A/B pairs against the generic build, native faster in all five (median 0.46 s vs 0.50 s on average). The board's own scan finds the flag, so this track is built on the scoring box, as the harness README says. The generic build is the fallback if that ever matters | | 0.46 | 53 | |

Step 0 is the scoring box's calibration of the reference on seed 1. Steps 1 and later are measured on my dev machine (4 vCPU Xeon @ 2.1 GHz, warm page cache), not the scoring box, and have no board run ID yet, so replace them with the board's numbers. T is the median of 5 runs on `full`, and the same binary varies about 10% between sessions on this shared VM, so only compare rows measured back to back (seed 1) and M is the peak anonymous RSS, sampled from `/proc/<pid>/status` (RssAnon), which leaves out the file pages of the mapped input. Every row's output hash matched on tiny, sample and full. This machine has 4 CPUs, so the 12-thread step cannot show its full gain here.
