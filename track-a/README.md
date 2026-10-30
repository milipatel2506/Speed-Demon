# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

A plain single-threaded Rust program. It reads `requests.log` into memory, splits each line on spaces and groups by (first 13 bytes of the timestamp, endpoint) in a `BTreeMap`, whose key order is already the required output order. Each group keeps its latencies and its 5xx count. At the end each group's latencies are sorted and the nearest-rank p50, p95 and p99 (`(p*n + 99) / 100`) and `errors * 1_000_000 / count` are printed in integers. Nothing relies on the input being sorted. This is the unoptimised baseline.

## Dependencies

None. Standard library only.

## With another 24 hours

Parse timestamps and numbers by hand from bytes instead of through `str`, swap the `BTreeMap` for a hash map and sort once, and split the file across threads with per-thread tables that are merged at the end.
