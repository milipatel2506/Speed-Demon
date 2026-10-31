# Track A: Ingest (team InnoTech)

Task and exact output format: the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md). The full log of what was tried, with numbers and the reason each failed experiment failed, is in [`PERF.md`](PERF.md).

Final board result: T 0.0520 s, M 53.7 MB, S 293, K 25.86 (the reference takes 32.05 s and 228 MB).

## Approach

One pass over `requests.log`, split across all available CPUs, then one parallel render. Nothing relies on the input being sorted.

1. **Map and cut.** The file is memory-mapped read-only and cut after a newline into one chunk per thread.
2. **Scan (parallel, one thread per chunk).** Each line is parsed in a single forward pass: the timestamp has a fixed width, the endpoint's end is found eight bytes at a time (XOR with spaces, then a zero-byte test), the latency's digits are converted with a few multiplies, and only the first status digit is read (5 means an error). The hour number comes straight from the timestamp digits, and is reused while the first 13 timestamp bytes repeat.
3. **Group.** A group is (endpoint, hour). Each thread interns endpoints in an open-addressing table, so a group is the dense number `endpoint_id * hours + hour`. Per group it keeps a request count and a 5xx count in flat arrays, and pushes one packed `u64` per line (`group << 32 | latency`).
4. **Counting sort.** After its chunk, each thread turns the counts into offsets and places every latency in one flat array, so that `lat[start[g]..start[g + 1]]` holds group `g`'s latencies. There is no per-group allocation, and the row blocks are freed as they are consumed.
5. **Merge.** The endpoint names of all threads are deduplicated, sorted bytewise once and ranked; each thread gets a map from rank to its local id. No group is ever re-inserted or sorted.
6. **Render (parallel, a run of hours per thread).** For each endpoint in sorted order and each hour of the thread's run, the latencies of all threads are gathered and sorted, and the output is written as `hour endpoint count p50 p95 p99 errors error_ppm`. Percentiles are nearest rank, the value at rank `(p * n + 99) / 100`; `error_ppm` is `errors * 1_000_000 / count` in integers; there is no floating point. Each hour has its own buffer, and the buffers are written to `summary.txt` in order.

Safety fallbacks (all tested with hand-made files): a timestamp outside the spec's 72-hour window makes the run redo the scan with the hours actually present; the intern table doubles when half full, so any number of endpoints works; the last line may lack a newline; an empty file gives an empty output; an unreadable input or unwritable output prints one line (`error: <path>: <reason>`) and exits with code 1.

## Dependencies

None: standard library only (`dependency_count = 0`, so D = 1). The one `extern "C"` declaration is libc's `mmap`, which `std` already links; it is used in two small `unsafe` blocks that each carry a safety note.

## Build and run

```
cargo build --release --offline
./target/release/solution [DATA_DIR] [OUT_DIR]      # defaults: /data and /out
```

It reads `DATA_DIR/requests.log` and writes `OUT_DIR/summary.txt`. The Dockerfile builds the same thing offline from the pinned `rust:1-slim-trixie` image into a `debian:13-slim` runtime image (`python3 check.py a --data <dir> --expected <sha256>` from the repository root runs it with the board's settings). `.cargo/config.toml` sets `-C target-cpu=native`, so the board builds this track on the scoring box.

To test against the reference: generate data with `generators/a_ingest.py --seed N --scale {tiny,sample,full}` and compare the sha256 of the output with the one from `reference/a_ingest.py` on the same data; this was done for seeds 1 to 4 at all three scales.

## What I would try next

- The scan is bound by streaming the 239 MB input and extracting fields (about 47 ms of the roughly 75 ms scan phase on four cores). Delimiter search with AVX2 and a cheaper hash did not move it, so the next idea is to overlap work differently: prefetching ahead of the parser, or splitting the page-fault cost with huge pages on the mapping itself (only anonymous memory was tried).
- The layout for the scoring box's 12 SMT threads is untested locally. Many small chunks (V1, V2) lost on the board; a plain one-chunk-per-core layout with the remaining threads used only in the render is the obvious experiment.
- The render's gather and per-group sort cost about 30 ms of CPU. A merge of the threads' latency slices (each sorted in its own thread during the scan) could replace the sort.
- PGO builds offline in the allowlisted image and passes the gates (the Dockerfile is in `PERF.md`) but gave at most 1% locally; it may be worth one board run.
- Source size: S is 293 against the reference's 56, which costs about 22% of K; the parser could be made shorter at some cost in speed.
