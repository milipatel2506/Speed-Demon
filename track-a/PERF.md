# Track A: summary (one screen)

**Final board result:** T 0.0520 s, M 53.7 MB, S 293, D 1, K 25.86 (machine points 50 x log10(K) = 70.6, before the judge share). Reference: T 32.05 s, M 228 MB, S 56, D 1. Check: (32.05 / 0.0520)^0.5 x (228 / 53.7)^0.2 x (56 / 293)^0.15 = 25.9.

| Board row (numbers as reported from the board) | T, s | M, MB | S | K |
|---|---|---|---|---|
| An earlier row | 0.063 | 52 | 258 | |
| Best build before the last change (cec700a) | 0.05716 | 54.5 | 292 | 24.61 |
| V1: 12 workers, 96 chunks (reverted) | 0.05991 | 54.2 | 311 | 23.83 |
| V2: 6 workers, 96 chunks (reverted) | 0.06784 | 44.6 | 311 | 23.29 |
| **Final (hashed endpoint ranking, this submission)** | **0.0520** | **53.7** | **293** | **25.86** |

Run IDs in the tables below are blank: the board's run IDs were not visible from the development environment. All times below that are not board rows were measured on a 4-CPU VM (the scoring box has 12 threads on six SMT cores), as medians of interleaved runs on the `full` seed-1 data; since the VM drifts by 10% or more between sessions, later changes were timed as paired rounds against the previous binary, and a 41-round control of one binary against itself shows a noise of about +-10 ms (IQR).

**What worked (local, in order):**

| Change | Measured gain |
|---|---|
| Simple std-only Rust instead of the Python reference | 33.7 s to 1.90 s |
| `mmap` plus a byte parser (no `str`, no `datetime`) | 1.90 s to 1.39 s |
| `HashMap` instead of `BTreeMap`; sort the groups once | 1.39 s to 0.85 s |
| One thread per CPU, newline-aligned chunks | 0.85 s to 0.57 s |
| `-C target-cpu=native` | about 8% (0.50 s to 0.46 s) |
| FxHash-style hasher instead of SipHash | 0.503 s to 0.453 s |
| Interned endpoints, flat per-thread arrays, counting sort, ranked output | 0.466 s to 0.319 s |
| Render on all threads | 0.325 s to 0.220 s |
| One forward scan per line | 0.206 s to 0.136 s |
| Eight bytes at a time for the endpoint scan | 0.135 s to 0.114 s |
| Packed `u64` rows freed block by block (memory) | peak 57 MB to 28 MB, no slowdown |
| Direct number formatting in the render | about 5% (0.110 s to 0.104 s) |
| Hour cache, SWAR latency parse, render loop interchange, no concatenation | wall -9 to -13 ms, cpu -30 to -36 ms (61 paired rounds) |
| Hash set and map for the endpoint-name ranking | ranking stage 8.3 ms to 3.8 ms with 12 workers |

**What failed, and why (all kept out of the code; details in the log below):**

| Experiment | Why it failed |
|---|---|
| V1 and V2 (96 chunks claimed by 12 or 6 workers, render tasks) | Board: slower than the one-chunk-per-thread build (K 23.83 and 23.29 against 24.61) and 19 more lines; V2 was a further 13% slower than V1 with half the threads |
| LTO, `codegen-units = 1`, `panic = "abort"` | No gain |
| Last-key cache, previous-endpoint check, cheaper endpoint hash | Consecutive lines rarely share an endpoint; the hash multiplies hide behind the delimiter search |
| `MAP_POPULATE`, `madvise`, huge-page tunable, `pread` instead of `mmap` | Page cache is warm and the faults are taken in parallel; each was a tie or slower |
| AVX2 delimiter search | Tie, 52 more lines; field extraction is bound by streaming the input, not by compares |
| Two-pass layout without the row buffer | 31% slower: the second pass re-parses every line |
| Percentile selection, parallel `pwrite`, early `exit`, pre-sized output file, reserved output buffers | Ties, or CPU saved that wall time did not show |
| More threads than CPUs, dynamic chunks | Oversubscription; load was already within 5% |
| PGO with build-time training (allowlisted llvm-tools image) | Builds offline and passes all gates, but at most 1% faster, inside the noise; not adopted |

Safety: the program does not rely on sorted input, on the 72-hour window (a slower second scan numbers the hours actually present), on at most 4,095 endpoints (the intern table doubles), or on a final newline; unreadable input or an unwritable output gives a one-line error and exit code 1. Verified against the reference hashes on seeds 1 to 4 at tiny, sample and full scale, the spec's worked example, empty input, three hand-made stress files (576 hours, 5,000 endpoints, no final newline) at 1 and 4 CPUs, builds with 1, 6 and 12 workers forced, and the board's own `run_submission.py` gates.

---

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
| 16 | Line count step 1: `render` becomes a closure inside `main` (no five-parameter signature or call site) and the chunk cuts become one iterator. `main.rs` goes from 279 to 262 code lines after rustfmt. Speed and memory unchanged: 25-run medians 0.123 s to 0.121 s and 0.118 s to 0.112 s, peak anon 29.4 MB to 30.2 MB (noise); all hashes and the three stress files pass | | 0.12 | 30 | |
| 17 | Line count step 2: inline `hash`, `percentile` and `hour_label` at their single use and shorten the `mmap` call. `main.rs` goes from 262 to 251 code lines after rustfmt. Speed unchanged (25-run medians 0.132 s to 0.128 s and 0.131 s to 0.133 s); hashes and the three stress files pass | | 0.13 | 29 | |
| 18 | Result of the memory and line-count work: peak anonymous memory 56.6 MB to 28.8 MB; `full` median 0.140 s to 0.136 s over 25 interleaved runs against the build before this work (min 0.122 s to 0.110 s, so no slowdown); `main.rs` 272 to 251 code lines after rustfmt. Image built offline and ran under the board limits, all three hashes matched, `full` took about 0.4 s wall including Docker start-up | | 0.14 | 29 | |
| - | Failed experiment (not kept, commit 67c4db9 reverted): per-thread `pread` (`FileExt::read_at`) in 64 KB blocks into one reused buffer, with a partial last line carried to the next block, instead of the `mmap`. Correct (all hashes, the stress files, 512-byte blocks), but a tie on speed: 25 interleaved runs against the mmap build gave median 0.112 s to 0.112 s and 0.111 s to 0.109 s (block sizes 64 KB and 256 KB tied, 1 MB slower). It cost 14 more code lines (251 to 265), gave no gain in anonymous memory (28.4 MB to 30.1 MB), and the 230 MB of file pages it avoided are not charged to the run anyway because the harness prewarms the page cache. It did not beat the best board run, so the mmap version is kept | | 0.11 | 30 | |
| - | Failed experiment (parse step 1, AVX2 delimiter search; `std::arch::x86_64`, `is_x86_feature_detected!`, the word-at-a-time code kept as the fallback). Variant A: a `find(byte)` helper for the end of the method and of the endpoint, 32 bytes per compare with `_mm256_cmpeq_epi8` and `_mm256_movemask_epi8`, then the copy-based `fold` hash: slower, 25-run medians 0.112 s to 0.127 s. Variant B: one 32-byte compare giving both spaces from a single bitmask (the second set bit is the end of the endpoint, with a continued search for the rare long endpoint) and a direct word-load hash: a tie, 0.104 s to 0.105 s (mean 0.108 s both), and 52 more code lines. All hashes and stress files passed. An ablation that keeps only the field extraction (delimiters, digits, hash, hour number) and skips the interning, counting and row buffer makes the whole program take about 0.045 s against about 0.105 s, so delimiter search is under half of the parse and is limited by streaming the 239 MB input and by the table updates, not by compare instructions. Reverted | | 0.11 | 30 | |
| - | Failed experiment (parse step 2, two-pass layout): count per group in a first pass over the chunk, then run a second pass over the same chunk that writes each latency straight to its final slot, so the row buffer disappears. All hashes and stress files passed, but it is much slower, 25-run medians 0.114 s to 0.150 s (about 31% slower), because the second pass re-parses and re-interns every line, which costs more than the row writes and the counting sort it replaces. Peak anonymous memory only went 29.0 MB to 27.0 MB, since the 12.4 MB latency array and the count tables still exist and the rows were already freed block by block. Reverted | | 0.15 | 27 | |
| 20 | Render step 1: write each summary line with direct appends instead of `write!`/`writeln!` (label, name, then the six numbers through a small `push_decimal` that fills a stack buffer), so no `core::fmt` machinery runs per line. An ablation first showed formatting is about 18 ms and the per-group sort 10 to 16 ms of the 110 ms run. 25-run medians against the previous build, two rounds: 0.110 s to 0.104 s and 0.108 s to 0.104 s (means 0.117 s to 0.107 s and 0.112 s to 0.108 s). A variant with a two-digit pair table (11 more lines) was no better (0.114 s to 0.116 s, then 0.109 s to 0.102 s and 0.113 s to 0.103 s) and was dropped. Costs 13 lines (251 to 264). Hashes and stress files pass | | 0.10 | 29 | |
| - | Failed experiment (render step 2): reserve each render thread's output buffer up front (range length x endpoints x 80 bytes, which is nearly exact because almost every pair has a line) instead of letting it grow. A tie, 25-run medians 0.121 s to 0.120 s and 0.124 s to 0.120 s (means 0.122 s to 0.118 s and 0.118 s to 0.119 s), peak anon 28.6 MB to 28.7 MB, 2 more lines. Reverted | | 0.12 | 29 | |
| 21 | Result of this round (parse and render work; pread was reverted first): the only kept change is the direct number formatting in the render (row 20). Parse changes (AVX2 delimiter search, two-pass layout) and the output reserve did not help and are logged above. Whole program on `full`, 25 interleaved runs against the build before this round: median 0.110 s to 0.110 s and 0.109 s to 0.105 s, min 0.098 s to 0.093 s and 0.096 s to 0.094 s; the render phase went from about 30 ms to about 21 ms in the phase timers, but the parse variation between sets (60 to 80 ms with identical code) is larger than the whole-run gain, so the honest figure is a gain of a few percent. Peak anonymous memory 29.6 MB to 29.6 MB. `main.rs` is 264 code lines after rustfmt (251 before). The board's own run_submission.py gates all pass (correctness, timeout, sandbox) on sample and full | | 0.105 | 30 | |
| 22 | Field extraction step 1a: reuse the previous line's hour number when its first 13 timestamp bytes (as one fixed-size array compare) are equal, so the date arithmetic runs only when the hour changes. Paired interleaved runs against the previous build, 41 rounds each (identical-binary control: wall diff about -1 ms, IQR -13..+5, cpu diff about -3 ms): wall median diff -1.9, -2.4 and -3.0 ms, cpu -5.0 ms, new faster in 23 to 27 of 41 rounds. A small gain at the edge of the noise, kept for 4 lines (264 to 268) | | 0.10 | 29 | |
| 23 | Field extraction step 1b: parse the latency with SWAR. One 8-byte load finds the newline, the digits (up to seven) are shifted to the top of the word and converted with three multiplies (`digits8`); latencies of eight digits or more, and the end of a chunk, keep the byte loop. The status needs no parsing: only its first digit (a 5xx test) is read. Paired runs against 1a, 41 rounds each, two repeats: cpu -17.2 and -18.1 ms (about 5.5%, new faster in 37/41 and 33/41 rounds), wall -3.7 and -3.5 ms. Kept for 16 lines (268 to 284) | | 0.10 | 29 | |
| - | Failed experiment (field extraction step 1c): hash only the endpoint's length and its first and last eight bytes (zero-padded when shorter), with the full byte comparison against the stored name still deciding a match, and let the delimiter search loop run without hashing. Correct (all hashes and stress files) and one line shorter, but not faster: paired runs against 1b, 41 rounds each, two repeats: wall diff -1.7 and -1.3 ms, cpu -0.8 and -1.4 ms, new faster in 22/41 rounds every time. The per-word multiplies are hidden behind the delimiter search, so removing them saves nothing measurable. Reverted | | 0.10 | 29 | |
| - | Failed experiment (page-fault step, first and only one tried): GLIBC_TUNABLES=glibc.malloc.hugetlb=1 so that malloc asks for transparent huge pages (THP is set to madvise on this machine; no code, one ENV line). Faults 16,832 to 15,008 (the 512 KB row blocks are too small for 2 MB pages), but no speed gain: paired runs through an identical sh wrapper, 41 rounds, two repeats: wall +0.5 and -0.7 ms, cpu +6.8 and -0.1 ms, new faster in 19 to 22 of 41 rounds; peak anon 28.3 MB to 29.8 MB. Stopped the page-fault stage here as agreed because the first experiment showed no gain; the other ideas (a flat row buffer sized from the chunk, recycling the sort output) were already measured as ties or worse in earlier rows | | 0.10 | 29 | |
| 24 | Render step 1: loop interchange. The render now walks endpoint by endpoint with the hours of its range inside, so each part's counts (index id * hours + hour) are read in consecutive order instead of 288 bytes apart, and writes each hour's lines into its own buffer that are concatenated at the end to keep the output order. Paired runs against 1b, 41 rounds each, two repeats: cpu -6.9 and -7.6 ms (new faster in 32/41 rounds both times), wall -4.4 and -3.2 ms. Kept for 1 line (284 to 285) | | 0.095 | 29 | |
| - | Failed experiment (render step 2): find the three percentiles with `select_nth_unstable` (p99, then p95 below it, then p50 below that) instead of sorting each group; an ablation without the sort showed the sort costs about 31 ms of cpu and 11 ms of wall. Correct, but only a small wall gain: paired runs against the interchange build, 41 rounds, two repeats: cpu -8.6 and -6.3 ms, wall -2.0 and -0.6 ms (new faster in 25/41 and 21/41 rounds), and 10 more lines after rustfmt (285 to 295). The extra cpu saved is absorbed by the other render phases and the write, so wall time hardly moves. Reverted | | 0.095 | 29 | |
| 25 | Render step 3 (memory): return each thread's per-hour buffers and write them to the file in order, instead of concatenating them first, so the output is no longer held twice. Peak anonymous memory 32.0 MB to 29.8 MB (median of 9 sampled runs; the interchange build had raised it from 28.3 MB). Paired runs against the interchange build, 41 rounds, two repeats: wall -2.6 and +1.5 ms, cpu -11.4 and -4.3 ms, so no slowdown. Kept; same line count (285) | | 0.09 | 30 | |
| 26 | Result of this round (layers that dominate: field extraction, page faults, render). Kept: hour cache (1a), SWAR latency parse (1b), render loop interchange, direct write of the per-hour buffers. Not kept: cheaper endpoint hash (1c), glibc huge-page tunable (page-fault stage stopped after its first experiment), percentile selection. Against the build at the start of the round, paired interleaved runs, 61 rounds each, two repeats: wall median diff -12.8 and -8.9 ms (new faster in 50/61 and 49/61 rounds; medians 113.9 to 101.5 ms and 106.9 to 95.3 ms, min 93.6 to 86.1 ms and 93.7 to 85.3 ms), cpu -36.3 and -29.5 ms (57/61 and 58/61 rounds). The phase timers show parse about 64 ms to about 57 to 63 ms and render about 21 ms to about 20 to 22 ms; the paired whole-run numbers are the reliable ones because the VM drifts by 10% or more between sets. Peak anonymous memory 28.5 MB to 30.2 MB (the per-hour buffers cost about 2 MB more than the old single buffer). `main.rs` is 285 code lines after rustfmt (264 before; 21 added). Every kept step passed the three seed-1 hashes, seeds 2, 3 and 4, the spec example, empty input, the three stress files and the one-line file at 1 and 4 CPUs, and the board's run_submission.py gates | | 0.09 | 30 | |
| - | Failed experiment (Stage 4, PGO with build-time training in the allowlisted halloumee llvm-tools image): verified to build offline and to pass every check and the board's gates, but no measurable gain over the plain build from the same image (121 paired rounds: wall -0.9 ms, cpu -0.9 ms, better in 65/121 against 60/121 for the identical-binary control). Details, the verified Dockerfile and the rules reading are in the Stage 4 section. Not adopted; the Dockerfile and code are unchanged | | 0.09 | 30 | |
| - | Failed experiment, board-tested (variant V1, commit 206133a, reverted): 96 small chunks and 24 three-hour render tasks claimed from shared atomic counters by min(12, cpus) workers, one set of tables per worker, merged at the end. Board result: T 0.05991 s, M 54.2 MB, S 311, K 23.83. Against the best build cec700a (T 0.05716 s, M 54.5 MB, S 292, K 24.61) it is slower by 4.8%, uses the same memory, and costs 19 more code lines, so K is lower. The 12-worker, many-chunk layout did not pay on the box. Possible causes, not measured: twelve parts make the render gather more work per group, and the 24 small render tasks read each part's counts in shorter runs than the tip's one run of hours per thread; the extra 19 lines also cost K directly. Locally (4 CPUs) it had been a wall tie and +17 ms cpu. Reverted by a new commit, no force push | | 0.0599 | 54.2 | |
| - | Failed experiment, board-tested (variant V2, commit c9e1c18, reverted): the V1 layout with the worker cap at 6 instead of 12 (96 chunks and 24 render tasks claimed from shared counters). Board result: T 0.06784 s, M 44.6 MB, S 311, K 23.29, against the best build cec700a (T 0.05716 s, M 54.5 MB, S 292, K 24.61). Slower than both V1 and the best build; the memory is lower, but not enough to pay for the time. Reverted by a new commit, no force push | | 0.0678 | 44.6 | |
| 30 | Serial-path step (kept): endpoint-name ranking with a HashSet and a HashMap instead of a 22k-name sort and a binary search per name and part. Ranking stage 8.28 ms to 3.78 ms with 12 workers forced; whole-run paired timing wall -1.2 and -1.3 ms (12 workers forced), -1.6 ms (4 workers). Three other candidates (exit without dropping, parallel pwrite, pre-sized output file) were tried and did not help; see the serial-path audit. Verified: three seed-1 hashes, seeds 2, 3 and 4, spec example, empty input, the three stress files at 1 and 4 CPUs, the board's run_submission.py gates, and a build with 12 workers forced. 285 to 286 code lines (+1) | | 0.057 | 54 | |
| 31 | Final polish (no change in behaviour or speed): I/O failures now print one line and exit with code 1 instead of panicking (missing or unreadable input, a directory in place of the file, an unwritable output; a failed `mmap` is checked, which it was not before), `available_parallelism` falls back to one thread instead of panicking, each `unsafe` block has a safety note, the hash and the word-at-a-time scan are explained, the redundant length test in the latency parse and the `loop`/`break` in `push_decimal` are gone, and the chunk cuts are one iterator chain. `main.rs` 286 to 288 code lines after rustfmt (+2). Re-verified after the edit: seeds 1 to 4 at tiny, sample and full scale against the reference hashes, spec example, empty input, the three stress files and the one-line file at 1 and 4 CPUs, builds with 12, 6 and 1 workers forced, the board's run_submission.py gates; 41 paired rounds against the previous build: wall -0.5 ms (a tie) | | 0.052 | 53.7 | |

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
| Same session, before the memory work, set 1 | 0.02 | 86.7 | 3.1 | 36.2 | 6.0 | 131.8 |
| Same session, final build, set 1 | 0.02 | 82.1 | 2.8 | 36.2 | 6.8 | 129.1 |
| Same session, before the memory work, set 2 | 0.02 | 82.5 | 3.1 | 31.0 | 6.0 | 121.2 |
| Same session, final build, set 2 | 0.02 | 81.7 | 3.0 | 34.7 | 7.4 | 126.6 |
| Same session, before this round, set 1 | 0.01 | 72.6 | 2.6 | 32.3 | 6.7 | 112.4 |
| Same session, after (render step 1), set 1 | 0.01 | 59.8 | 2.6 | 20.7 | 6.8 | 90.5 |
| Same session, before this round, set 2 | 0.01 | 79.4 | 2.8 | 29.5 | 6.7 | 115.3 |
| Same session, after (render step 1), set 2 | 0.01 | 63.6 | 2.7 | 22.1 | 6.4 | 106.8 |
| Same session, before this round, set 1 | 0.01 | 64.2 | 2.6 | 21.8 | 5.9 | 94.5 |
| Same session, after this round, set 1 | 0.01 | 63.1 | 2.6 | 19.6 | 6.2 | 91.9 |
| Same session, before this round, set 2 | 0.01 | 62.9 | 2.7 | 20.3 | 5.9 | 91.9 |
| Same session, after this round, set 2 | 0.01 | 57.4 | 2.8 | 21.9 | 6.6 | 88.7 |

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

After the memory changes (same run, same sums over 4 threads): `rows` 24.8 MB in 64K-entry blocks freed as the counting sort consumes them (was 49.6 resident), `lat` 12.4 MB (unchanged), `start` 2.1 and `errors` 2.1 (the `next` clone is gone), `slots` 0.07, output pieces 10.1 (live only after the scan). Measured peak anonymous memory: **56.6 MB to 28.8 MB** (median of 9 sampled runs; max 60.8 MB to 29.6 MB).

### Thread and merge overhead (full, 4 threads, scratchpad timers, 5 runs)

Measured to check that the fixed costs are small; no code changed.

| Cost | Median, ms | Max, ms |
|---|---|---|
| Spawn loop for all scan threads | 0.19 | 0.28 |
| A scan thread's start latency after spawning began | 0.39 | 7.3 (one scheduling outlier in 20) |
| Counting sort alone, per thread (includes allocating its output) | 7.2 | 8.5 |
| A whole scan thread, counting sort included | 82.6 | 105.4 |
| Rank endpoint names / map thread ids (the whole merge) | 2.6 | |

Spawning, start-up and the merge cost well under 1% of the run between them. The counting sort is about 9% of each scan thread, but it is real work (3.1M scattered writes), not overhead.

### Stage 1: where the time goes, layer by layer (full, seed 1, 4 threads)

Cumulative ablation builds of the current source (scratchpad only, outputs deliberately wrong), each stopping after one more layer. "Level" is the median time of the scan phase (`scan_all`, ms); "step" is the median of the per-round difference to the previous layer over 41 interleaved rounds, which cancels the drift of this shared VM. Single medians of two separate sessions disagreed by up to 15 ms per layer, so read each step with its spread.

| Layer (cumulative) | Level, ms | Step, ms | Step spread (p25 to p75) | Minor page faults added |
|---|---|---|---|---|
| Input streaming only (count newlines over the mapping) | 17.0 | 17.0 | 16.0 to 18.4 | 3,523 |
| + field extraction: delimiters, endpoint hash, digits, hour number | 47.9 | 30.1 | 22.9 to 32.8 | about 0 |
| + intern lookup (open-addressing probe, name compare) | 57.3 | 10.3 | 6.8 to 13.2 | 1,257 |
| + count and error updates (`start[g] += 1`, `errors[g] += e`) | 57.3 | 3.8 | -1.3 to 6.2 | 2 |
| + `u64` row push (64K-entry blocks) | 70.1 | 9.4 | 5.1 to 15.0 | 6,076 |
| + counting sort | 74.7 | 6.1 | -1.7 to 15.0 | 3,052 |

Outside the scan, for the whole program (phase timers, two sets, ms): mmap call 0.01, rank endpoint names 2.7 to 2.8, render 23.2 and 27.9, write 6.4 and 6.1, total 111.8 and 125.4.

What the ladder says:

- Streaming plus field extraction is about 47 ms of the roughly 75 ms scan: it is the largest cost and the one that scales with the input size. The AVX2 experiment (rows above) moved only the delimiter part of it and got nothing.
- The random count and error updates cost about 4 ms (at most 6) and the whole counting sort about 6 ms (the single measurements range from 4.6 to 12). Together that is roughly 10 ms of a 110 ms run, which is the most a radix sort could ever remove, and a radix sort adds its own two passes over 25 MB plus a second 25 MB buffer (more page faults), so it is unlikely to win.
- The row push costs about 7 ms when rows are leaked instead of freed (9.4 ms paired step; 7.1 ms with rows leaked), so freeing them is only about 2 ms. Pushing into a flat, pre-faulted buffer made the line loop itself free (60.2 ms against 60.8 ms for the counts-only layer), so the row cost is page faults and block allocation (6,076 faults), not stores.
- Render is the second largest phase at about 23 to 28 ms (gather, sort of each small group, formatting), then the write at about 6 ms.
- Page faults are a visible cost on this VM: 3.5k for the input mapping and 10.4k for the row and latency buffers, at an estimated 1 to 2 microseconds each.

Verification pipeline for the stages that follow: the three seed-1 hashes, seeds 2, 3 and 4 at full scale (reference hashes in the scratchpad), the spec example, empty input, the three stress files plus the one-line file at 1 and 4 CPUs, and the board's own `run_submission.py` gates; all pass on the current build.

### Stage 4: PGO (profile-guided optimisation), tried and not adopted

What the board does: nothing PGO-specific. It runs `docker build --network=none` on `track-a/Dockerfile`, so the whole profile cycle has to happen inside RUN steps, offline, using an allowlisted image that has `llvm-profdata`: `halloumee/speed-demon-rust:1.99.1-trixie-llvm-tools@sha256:13e4ee71...` (rust:1-trixie plus rustup's llvm-tools; rustc 1.99.1 on LLVM 23.1.1 and llvm-profdata 23.1.1-rust-1.99.1, so the tool always matches the compiler; it also has python3). The slim image in the current Dockerfile has the same rustc but no llvm-profdata.

Rules and S: the harness README and `static_axes.py` say a published generator copied in "for build-time training (a JVM AOT cache, PGO)" costs nothing, so a byte-identical copy of `generators/a_ingest.py` in the track folder is listed under not_counted. A committed `.profdata` would also not count (an unrecognised binary file is sniffed and listed as "binary", 0 lines), and nothing in the repo forbids it, but only the build-time route is named in the docs, and it avoids any version mismatch (a profile must come from the llvm-profdata of the compiler that uses it: the system LLVM 18 tool here could not read a rustc 1.97 / LLVM 22 profile). The site's own rules are not in the repo, so ask the organisers before committing a profile.

Verified Dockerfile (builds offline, about 15 s, gates pass; needs `a_ingest.py` copied byte-identically from `generators/` into `track-a/`):

```
FROM halloumee/speed-demon-rust:1.99.1-trixie-llvm-tools@sha256:13e4ee7135d8eac6c56476c7d1aee201f36479f4745fd5d0e6b4529f4e13251a AS build
WORKDIR /src
COPY . .
RUN RUSTFLAGS="-C target-cpu=native -C profile-generate=/tmp/pgo" cargo build --release --offline --target-dir /tmp/instrumented \
 && python3 a_ingest.py --seed 1 --scale full --out /tmp/train \
 && mkdir /tmp/train-out \
 && /tmp/instrumented/release/solution /tmp/train /tmp/train-out \
 && llvm-profdata merge -o /tmp/merged.profdata /tmp/pgo \
 && RUSTFLAGS="-C target-cpu=native -C profile-use=/tmp/merged.profdata" cargo build --release --offline

FROM debian:13-slim@sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad
COPY --from=build /src/target/release/solution /app/solution
ENTRYPOINT ["/app/solution"]
```

(`RUSTFLAGS` replaces `.cargo/config.toml`'s rustflags, so `target-cpu=native` is repeated.) Result: the image is already in this sandbox's Docker store (the registry still answers 429), so it was used by digest. The PGO binary matches the three seed-1 hashes, seeds 2, 3 and 4, the spec example, empty input and the three stress files, and the board's `run_submission.py` gates pass on the PGO Dockerfile (sample and full, 3 runs each). But it is not faster: paired interleaved runs against the plain build from the same image, wall median diff -2.5 and -2.0 ms over 61 rounds each (new faster in 35/61 and 38/61), and -0.9 ms over 121 rounds (65/121 against 60/121 for the control of one binary against itself), cpu -3.7, -7.7 and -0.9 ms. That is inside the +-10 ms noise of this VM, so the gain is at most about 1%. Peak anonymous memory 29.0 MB to 29.6 MB. Not adopted: it would add a 508-line Python file to the folder and a longer, riskier build (the build must run a training pass; if the box's builder ever lacked python3 or a writable /tmp, the build would fail) for no measurable gain.

### Board results for the thread-layout variants (V1 and V2), and what they say about the serial work

| Build | Workers | Chunks | T, s | M, MB | S | K |
|---|---|---|---|---|---|---|
| Best build cec700a | 4 to 12 (one chunk per thread) | one per thread | 0.05716 | 54.5 | 292 | 24.61 |
| V1 (206133a, reverted) | 12 | 96 | 0.05991 | 54.2 | 311 | 23.83 |
| V2 (c9e1c18, reverted) | 6 | 96 | 0.06784 | 44.6 | 311 | 23.29 |

Observation reported with the board results: going from 6 to 12 workers cut the time by only about 12% (0.06784 s to 0.05991 s), which suggests that roughly half of the wall time is serial or fixed work; and the 9.6 MB memory gap (54.2 MB to 44.6 MB) implies about 1.6 MB per worker thread. A caveat on the first inference: twelve SMT threads on six cores deliver much less than twice the compute of six threads (typically 20 to 30% more), so a 12% gain is also consistent with a mostly parallel run; an Amdahl fit through the two timings alone (T = serial + parallel / workers) would put the serial part at about 52 ms, which is an upper bound, not a measurement. The serial path is therefore timed directly in the next section instead of being inferred.

### Serial-path audit with 12 workers (stamps on CLOCK_MONOTONIC in a scratchpad copy, median of 21 runs, ms)

The scratch copy is forced to 12 workers on this 4-CPU VM, so the parallel stages (scan, render) and the two thread spawns are inflated by oversubscription and by the spawning thread being descheduled (their minimums are 0.2 to 1 ms); read the serial stages, not those. "Before" is the best build cec700a; "after" has the hashed ranking below.

| Stage | Before | After |
|---|---|---|
| process start to main (fork, exec, loader, runtime) | 1.47 | 1.44 |
| args, open and mmap of the input | 0.02 | 0.02 |
| chunk cuts (newline search at 12 cut points) | 0.04 | 0.04 |
| hour labels + spawn of 12 scan threads | 4.71 | 9.23 |
| parallel scan until the last thread finishes | 62.79 | 56.84 |
| join returns after the last thread finished | 0.08 | 0.07 |
| collect of parts | 0.00 | 0.00 |
| endpoint-name ranking and maps, total | 8.28 | 3.78 |
| spawn of 12 render threads | 5.72 | 0.59 |
| parallel render until join | 20.88 | 26.03 |
| output file create and write | 7.31 | 6.61 |
| drops of all buffers at the end of main | 2.36 | 2.26 |
| process exit (teardown, until the parent sees it) | 3.28 | 3.34 |
| TOTAL | 115.07 | 111.85 |

Adding up the serial stages (start-up, open, cuts, ranking, write, drops, exit, plus the two spawns at their typical cost) gives roughly 20 to 35 ms of the 112 to 115 ms total here, not half, but it is a larger share on the box where the parallel stages are shorter; the two-timing Amdahl fit in the previous section remains an upper bound.

Per-worker memory (peak anonymous, median of 9 sampled runs, with the worker count forced): 4 workers 29.6 MB, 6 workers 32.7 MB, 12 workers 43.0 MB, i.e. 1.55 MB per worker from 4 to 6 and 1.7 MB per worker from 6 to 12, matching the 1.6 MB read off the board. About 1.06 MB of that is the two per-worker count tables (`start` and `errors`, 132k `u32` each); the rest is the worker's last partial row block, its intern table and names, and its allocator arena and stack. The board's 54 MB at 12 workers is higher than this because it also charges the 10 MB output file's page cache.

Candidates from the audit:

- Kept: endpoint-name ranking with a `HashSet` (dedupe) and a `HashMap` (name to rank) instead of sorting all 22k names and binary-searching each of them for every part. The ranking stage goes from 8.28 ms to 3.78 ms (8.10 ms to 4.00 ms in an earlier pair of audits; minimum 7.7 ms to 3.2 ms). Whole-run paired timing is too noisy to see 4 ms: forced 12 workers, 41 rounds twice: wall -1.2 and -1.3 ms; real 4 workers: wall -1.6 ms, cpu -7.5 ms. One more line (285 to 286). Kept.
- Failed: `std::process::exit(0)` right after the last write, to skip freeing every buffer. The drop stage (2.4 to 2.8 ms) just moves into the kernel's teardown of the address space: paired wall +0.6 and -1.3 ms (forced 12 workers, 41 rounds each) and +0.2 ms (real 4 workers), new faster in 20 to 22 of 41 rounds.
- Failed: writing the per-hour buffers in parallel with `write_all_at` at prefix-sum offsets after `set_len`, in 12 threads. The write stage stays at about 7 ms (7.01 ms to 7.22 ms) for 16 more lines; buffered writes to one file most likely serialise in the kernel (not verified).
- Failed: `set_len` of the whole output before the serial writes: 6.46 and 6.58 ms against 6.59 and 6.49 ms.
- Nothing to remove: no large allocation is zero-filled by the program (the latency array comes from `calloc`, whose pages the kernel zeroes on first touch and which are counted as page faults; the count tables grow by 72 entries per new endpoint).
- Not done: shrinking the per-worker count tables. Using `u16` would halve them but is wrong whenever one (endpoint, hour) group has more than 65,535 requests in one worker's share, and exact sizing does not help because the tables are dense (127k of the 132k slots are used).

PGO: see Stage 4 above (tried with the allowlisted llvm-tools image, no measurable gain).

Final check of the finished code: the image built offline with `--network=none` and ran under the board's limits (read-only root, 1 GB `/tmp`, uid 65534, `/data` read-only, 4 CPUs). `/out` held only `summary.txt`, and all three hashes matched. Full took 0.98 s wall including `docker run` start-up (about 0.5 s of that is Docker, not the program). The box has 12 threads, so expect the threaded scan to do better there than on this 4-CPU machine.
