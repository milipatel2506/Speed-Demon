# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

A std-only Rust program. It memory-maps `requests.log` (a small `extern "C"` declaration of `mmap`, no crate), cuts the file after a newline into one chunk per available CPU, and each thread scans its chunk with a hand-written byte parser: the hour is the first 13 bytes of the timestamp, and latency and status are parsed from digits. Each thread keeps a `HashMap` (with a small FxHash-style hasher instead of SipHash) from (hour, endpoint bytes) to its latencies and 5xx count. The tables are merged, the groups are sorted once by (hour, endpoint) as bytes, and each group's latencies are sorted for the nearest-rank p50, p95 and p99 (`(p*n + 99) / 100`). `error_ppm` is `errors * 1_000_000 / count` in integers. Nothing relies on the input being sorted. Built with `-C target-cpu=native`. `PERF.md` has every step, including the experiments that did not help.

## Dependencies

None. Standard library only.

## With another 24 hours

Intern the 1,842 endpoints once so groups become a flat array indexed by hour and endpoint id, merge the thread tables in parallel, and try PGO with a profile generated inside the allowlisted llvm-tools image.
