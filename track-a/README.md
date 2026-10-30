# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

A std-only Rust program. It memory-maps `requests.log` (a small `extern "C"` declaration of `mmap`, no crate), cuts the file after a newline into one chunk per available CPU, and each thread scans its chunk with a hand-written byte parser. A thread interns each endpoint into a 4096-slot open-addressing table (FxHash-style hash), so a group is a dense (endpoint id, hour number) pair. The hour number comes straight from the timestamp digits, using the spec's fixed 72-hour span. Per-thread flat arrays hold the request and 5xx counts, and the latencies are counting-sorted by group, so no per-group allocation exists. Merging is ranking the endpoint names once, bytewise, and mapping each thread's ids to ranks. The output is then rendered hour by hour over the sorted names, on all threads at once: for each group the threads' latency slices are gathered, sorted, and the nearest-rank p50, p95 and p99 (`(p*n + 99) / 100`) and `errors * 1_000_000 / count` are printed in integers. Nothing relies on the input being sorted. Built with `-C target-cpu=native`. `PERF.md` has every step, including the experiments that did not help.

## Dependencies

None. Standard library only.

## With another 24 hours

Speed up the parse, which is now about 80% of the run (it splits and re-scans each line; a single pass over the bytes should be faster), and try PGO with a profile generated inside the allowlisted llvm-tools image.
