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
| 8 | Opt 8: render the output in parallel. Each thread gathers, sorts and formats a run of consecutive hours, and the pieces are written in order. Interleaved 15 runs vs opt 7: median 0.325 s to 0.220 s, min 0.262 s to 0.181 s | | 0.22 | 49 | |
| - | Failed experiment (opt 9): check whether this line's endpoint equals the previous line's before probing the intern table. Same hashes, no gain over opt 8 (median 0.225 s vs 0.235 s, mean 0.225 s both, 15 interleaved runs): consecutive lines rarely share an endpoint and the probe is already one hash and one compare. Reverted | | 0.24 | 55 | |
| 9 | Safety, no speed goal: the program no longer assumes the 72-hour window, 4,095 endpoints or a final newline. The hour index is checked against the window (year included) and any line outside it makes the run redo the scan with the hours actually present (their first 13 bytes, which sort chronologically); the endpoint table doubles when half full; and the last chunk cut no longer lands inside an unterminated last line. Tested against the reference on a 576-hour file with 2025 and 2027 dates, a 5,000-endpoint file and a one-line file, all without a final newline, at 1 and 4 CPUs. The old binary panicked, hung and mis-cut on those. Interleaved 15 runs vs opt 8 on `full`: median 0.234 s to 0.246 s, mean 0.241 s to 0.238 s, which is noise | | 0.24 | 52 | |
| 10 | Part 2 step 1: parse each line in one forward scan: jump past the fixed-width timestamp, skip the method, hash the endpoint byte by byte while finding its end, read the 5xx flag from the status's first digit, parse latency digits to the newline. No separate newline search or field split. Interleaved 15 runs vs row 9: median 0.206 s to 0.136 s, min 0.179 s to 0.119 s | | 0.14 | 58 | |
| 11 | Part 2 step 2: find the end of the endpoint 8 bytes at a time (a zero byte in the word xor spaces) and hash it a word at a time instead of a byte at a time; the last few bytes of a chunk use a scalar tail. Interleaved runs vs row 10: median 0.135 s to 0.114 s over 25 runs (0.130 s to 0.124 s over an earlier 15) | | 0.11 | 58 | |
| - | Failed experiments (Part 2 step 3), each against the step 2 binary over 15 interleaved runs, all with matching hashes: `MAP_POPULATE` (0x8002 flags) median 0.126 s to 0.134 s; `madvise(MADV_SEQUENTIAL)` 0.118 s to 0.123 s; `madvise(MADV_WILLNEED)` 0.123 s to 0.131 s. The page cache is already warm, so the 27,500 faults are minor faults that the 4 threads take in parallel inside the parse, and populating the mapping from one thread up front costs more than it saves. Reverted | | 0.13 | 58 | |
| - | Failed experiments (Part 2 step 4), all with matching hashes. Dynamic load balancing: 4 chunks per thread taken from a shared `AtomicUsize`, one table per thread spanning its chunks. Against the step 2 binary: median 0.131 s to 0.123 s in one 25-run comparison, then 0.126 s to 0.129 s and 0.115 s to 0.121 s in two more, so no real gain, and one chunk per thread through the same code (0.138 s vs 0.140 s) is a tie. Per-thread scan times with equal chunks differ by only about 5% (60 to 64 ms on 4 threads). Thread count: 8 and 12 threads on this 4-CPU machine are slower (0.150 s and 0.160 s against 0.138 s) because they oversubscribe it, which says nothing about the box: there `available_parallelism` already returns 12, so '12 threads' is the current behaviour. Reverted | | 0.12 | 58 | |
| 12 | Part 2 result, final build vs the Part 1 build, 15 interleaved runs on `full`: median 0.252 s to 0.138 s, min 0.214 s to 0.113 s. Kept: steps 1 and 2. Not kept: step 3 (`MAP_POPULATE`, `madvise`) and step 4 (dynamic chunks, 8 and 12 threads on 4 CPUs). Peak anonymous memory 49 to 58 MB. `main.rs` is 272 code lines after rustfmt. The image built offline and ran under the board limits with all three hashes matching (`full` 0.40 s wall including Docker start-up). The rows of the phase table measured in different sessions are not comparable with each other because the VM drifted by up to 30% (the same binary took 97 ms and 130 ms in total); the 'Same session' rows were taken back to back | | 0.14 | 49 | |
| 13 | Memory step 1: pack each line's (group, latency) into one `u64` (group << 32 | latency) instead of a padded 16-byte tuple, which halves `rows`. Peak anonymous memory 57.0 MB to 33.1 MB (median of 7 sampled runs); time is a tie to slightly better, median 0.137 s to 0.132 s over 15 interleaved runs | | 0.13 | 33 | |
| 14 | Memory step 2: drop the `next` clone in the counting sort by turning counts into end offsets and placing each latency just below its group's end (every slot then ends at its group's start offset). Saves 2 lines and about 0.7 MB. Time is a tie over three comparisons (15 runs 0.104 s to 0.113 s, then 25 runs 0.128 s to 0.126 s and 0.129 s to 0.126 s); peak anon 32.6 MB to 31.9 MB. Kept as a simplification | | 0.13 | 32 | |
| - | Failed experiment (memory step 3): preallocate `rows` with capacity chunk length / 40 (a line has at least 40 bytes) so it never reallocates. Same hashes; time is a tie (median 0.131 s to 0.129 s over 25 runs) and the peak does not move (31.4 MB to 31.0 MB), because untouched capacity is not resident. Reverted | | 0.13 | 31 | |
| 15 | Memory step 4: keep `rows` in 64K-entry blocks (`Vec<Vec<u64>>`) that the counting sort frees as it consumes them, so the rows and the latency output no longer coexist in full. Peak anon 31.4 MB to 27.9 MB (median of 7 sampled runs). Time is level to slightly slower (25 runs: median 0.129 s to 0.131 s and 0.127 s to 0.130 s, mean 0.126 s to 0.126 s and 0.125 s to 0.127 s), about 5 more lines. Kept because the memory axis gains more than the time axis loses (about +2% in K against about -1%), but it is a close call | | 0.13 | 28 | |

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

Opt 7 assumed the spec's fixed 72-hour span and at most 4,095 distinct endpoints. Row 9 removes both assumptions: the span is now only the fast path, and anything outside it is handled by a slower second scan.

### After opt 8 (parallel render), full, median of 5 runs, ms

| Phase | Opt 7, set 1 | Opt 7, set 2 | Opt 8, set 1 | Opt 8, set 2 |
|---|---|---|---|---|
| `mmap` call | 0.02 | 0.02 | 0.02 | 0.02 |
| Parallel parse (scan, intern, counting sort) | 191.5 | 206.2 | 193.3 | 149.2 |
| Rank endpoint names | 3.4 | 2.7 | 2.7 | 2.6 |
| Percentile sort and formatting | 108.5 | 100.9 | 37.3 | 34.9 |
| Write `summary.txt` | 7.0 | 6.8 | 6.8 | 5.8 |
| Total | 326.3 | 316.4 | 245.1 | 193.2 |

The render now runs on all 4 CPUs of this machine, so the 12 threads of the scoring box should shrink it further. The hours are cut into equal runs, so a thread with busier hours finishes last.

### Summary of the redesign (opts 7 and 8), full, 5 runs per set, ms

| Phase | Opt 6, set 1 | Opt 6, set 2 | Final, set 1 | Final, set 2 |
|---|---|---|---|---|
| `mmap` call | 0.02 | 0.01 | 0.02 | 0.02 |
| Parallel parse (final: includes intern and counting sort) | 221.5 | 240.5 | 197.0 | 208.5 |
| Merge tables / rank endpoint names | 96.4 | 102.7 | 3.0 | 2.7 |
| Sort the group list | 44.7 | 42.0 | none | none |
| Percentile sort and formatting (final: parallel) | 94.9 | 117.6 | 37.5 | 34.6 |
| Write `summary.txt` | 7.3 | 7.1 | 6.1 | 6.3 |
| Total | 483.5 | 505.5 | 248.4 | 259.0 |

Whole program, 15 interleaved runs, opt 6 binary against the final one: median 0.461 s to 0.252 s, min 0.378 s to 0.181 s. Peak anonymous memory about 44 to 56 MB. The parse is now about 80% of the run. `main.rs` is 195 code lines after rustfmt (it is committed already formatted; rustfmt widens long lines and small struct literals, so the raw count is 159).

### Phase table across the parse work (full, median of 5 runs, ms)

Temporary timers in a scratchpad copy, not in the repo; one set of 5 runs per row unless a label says otherwise, and this VM drifts by 10% or more between sets. Parse includes each thread's counting sort of its latencies.

| Version | `mmap` | Parse | Rank names | Render | Write | Total |
|---|---|---|---|---|---|---|
| Before (Part 1 build), set 1 | 0.02 | 162.2 | 2.7 | 30.6 | 5.9 | 208.2 |
| Before (Part 1 build), set 2 | 0.02 | 146.1 | 2.6 | 27.7 | 6.0 | 190.5 |
| Part 2 step 1: single forward scan, set 1 | 0.02 | 86.7 | 2.6 | 24.6 | 5.5 | 118.7 |
| Part 2 step 1: single forward scan, set 2 | 0.02 | 83.1 | 2.6 | 24.2 | 5.6 | 115.6 |
| Part 2 step 2: word-at-a-time endpoint scan and hash | 0.01 | 66.24 | 2.51 | 24.40 | 5.71 | 97.36 |
| Same session, Part 1 build, set 1 | 0.02 | 167.1 | 2.6 | 31.5 | 6.2 | 210.9 |
| Same session, final build, set 1 | 0.02 | 91.7 | 2.6 | 30.5 | 5.5 | 130.5 |
| Same session, Part 1 build, set 2 | 0.01 | 180.0 | 2.7 | 32.0 | 5.5 | 217.4 |
| Same session, final build, set 2 | 0.01 | 86.1 | 2.8 | 32.6 | 6.1 | 126.2 |

### Memory breakdown (full, seed 1, sums over the 4 threads)

Sizes printed from a scratchpad copy before any memory change. The peak is at the end of the scan, when each thread holds its `rows` (one entry per line) and the counting-sort output `lat` at the same time; the output pieces exist only after the rows are freed.

| Buffer | Before, MB | Note |
|---|---|---|
| `rows`: one (group, latency) entry per line | 49.6 resident, 67.1 capacity | a `(usize, u32)` tuple is padded to 16 bytes; 3.1M lines |
| `lat`: latencies after the counting sort | 12.4 | 4 bytes per line |
| `start`, `errors` and the `next` clone | 2.4 + 2.4 + 2.1 | one slot per (endpoint, hour) per thread |
| `slots` and `names` | 0.2 | |
| Output pieces | 10.5 | live only after the scan |
| Measured peak anonymous memory | 57.0 (median of 7 sampled runs) | |

Not tried: PGO. The system `llvm-profdata` is LLVM 18 but `rustc` here is LLVM 22, so the raw profile could not be merged, and a profile built here would not match the compiler in the allowlisted `halloumee/speed-demon-rust` image anyway. I could not pull that image because Docker Hub rate-limited the sandbox (HTTP 429). It needs a machine with that image, a committed `.profdata` generated inside it, and `-C profile-use` in the Dockerfile.

Final check of the finished code: the image built offline with `--network=none` and ran under the board's limits (read-only root, 1 GB `/tmp`, uid 65534, `/data` read-only, 4 CPUs). `/out` held only `summary.txt`, and all three hashes matched. Full took 0.98 s wall including `docker run` start-up (about 0.5 s of that is Docker, not the program). The box has 12 threads, so expect the threaded scan to do better there than on this 4-CPU machine.
