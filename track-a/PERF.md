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
| 6 | Opt 6: FxHash-style multiply-rotate hasher (`BuildHasherDefault<Fx>`, std only) in place of SipHash. Chosen from the phase table below. Interleaved 15 runs of each build: median 0.503 s to 0.453 s, min 0.433 s to 0.386 s | | 0.45 | 54 | |
| 7 | Opt 7: new data layout. Endpoints interned in a 4096-slot open-addressing table per thread (Fx-style hash, names stored once); a group is (endpoint id, hour number) with flat per-thread arrays for 5xx counts and a counting sort of the latencies by group, so there is no per-group allocation and no `HashMap`; merging is just ranking the endpoint names once (sorted bytewise) and mapping each thread's ids to ranks, then output iterates hour by hour over the sorted names, so the 127k groups are never sorted. Interleaved 15 runs vs opt 6: median 0.466 s to 0.319 s, min 0.393 s to 0.264 s | | 0.32 | 49 | |

Step 0 is the scoring box's calibration of the reference on seed 1. Steps 1 and later are measured on my dev machine (4 vCPU Xeon @ 2.1 GHz, warm page cache), not the scoring box, and have no board run ID yet, so replace them with the board's numbers. T is the median of 5 runs on `full`, and the same binary varies about 10% between sessions on this shared VM, so only compare rows measured back to back (seed 1) and M is the peak anonymous RSS, sampled from `/proc/<pid>/status` (RssAnon), which leaves out the file pages of the mapped input. Every row's output hash matched on tiny, sample and full. This machine has 4 CPUs, so the 12-thread step cannot show its full gain here.

## Where the time goes (full, seed 1, before opt 6)

Temporary timers, since removed, around each phase, 5 runs each, median per phase, in milliseconds. Two complete sets are shown because this shared VM drifts by 10% or more between sets. After opt 6, the same timers gave a set of 483 ms total and a later set of 548 ms, and the phase numbers moved by more than the change itself, so only the 15-run interleaved comparison above is evidence for the hasher.

| Phase | Before, set 1 | Before, set 2 | After opt 6, set 1 | After opt 6, set 2 |
|---|---|---|---|---|
| `mmap` call | 0.02 | 0.01 | 0.02 | 0.02 |
| Parallel parse (spawn, scan, join; includes the page faults) | 233.9 | 240.5 | 221.5 | 262.5 |
| Merge the per-thread tables | 128.1 | 102.7 | 96.4 | 99.3 |
| Sort the group list | 42.1 | 42.0 | 44.7 | 54.8 |
| Percentile sort and formatting | 91.0 | 117.6 | 94.9 | 118.2 |
| Write `summary.txt` | 6.2 | 7.1 | 7.3 | 6.8 |
| Total | 500.3 | 505.5 | 483.5 | 547.6 |

About 27,500 minor page faults happen, all inside the parse phase. The parse is roughly half the run, the merge and the percentile step about a fifth each, and the group sort about a tenth. The hasher touches the parse (3.1 million lookups) and the merge (about 500,000 re-inserts), which is why it was the candidate the timings justified. `MAP_POPULATE` and `madvise` were not tried: the `mmap` call itself costs nothing, and the faults are spread over 4 threads inside a phase that is mostly parsing, so the most they could save is a fraction of that half. The merge, the group sort and the percentile step are the next targets.

### After opt 7 (interned endpoints and flat storage), full, median of 5 runs, ms

Two sets, same drift caveat. The timers are in a scratchpad copy, not in the repo. Parse now includes each thread's counting sort of its latencies. The old merge and group-sort phases are gone: "rank" is naming and mapping the endpoints once.

| Phase | Opt 6, set 1 | Opt 6, set 2 | Opt 7, set 1 | Opt 7, set 2 |
|---|---|---|---|---|
| `mmap` call | 0.02 | 0.01 | 0.02 | 0.02 |
| Parallel parse (scan, intern, counting sort) | 221.5 | 240.5 | 191.5 | 206.2 |
| Merge tables / rank endpoint names | 96.4 | 102.7 | 3.4 | 2.7 |
| Sort the group list | 44.7 | 42.0 | none | none |
| Percentile sort and formatting (serial) | 94.9 | 117.6 | 108.5 | 100.9 |
| Write `summary.txt` | 7.3 | 7.1 | 7.0 | 6.8 |
| Total | 483.5 | 505.5 | 326.3 | 316.4 |

Opt 7 assumes the spec's fixed 72-hour span (every timestamp lies between 2026-10-30T18:00:00Z and 2026-11-02T17:59:59Z for every seed and scale, section 2 of the spec) and at most 4,095 distinct endpoints (the catalogue is 1,842).

Not tried: PGO. The system `llvm-profdata` is LLVM 18 but `rustc` here is LLVM 22, so the raw profile could not be merged, and a profile built here would not match the compiler in the allowlisted `halloumee/speed-demon-rust` image anyway. I could not pull that image because Docker Hub rate-limited the sandbox (HTTP 429). It needs a machine with that image, a committed `.profdata` generated inside it, and `-C profile-use` in the Dockerfile.

Final check of the finished code: the image built offline with `--network=none` and ran under the board's limits (read-only root, 1 GB `/tmp`, uid 65534, `/data` read-only, 4 CPUs). `/out` held only `summary.txt`, and all three hashes matched. Full took 0.98 s wall including `docker run` start-up (about 0.5 s of that is Docker, not the program). The box has 12 threads, so expect the threaded scan to do better there than on this 4-CPU machine.
