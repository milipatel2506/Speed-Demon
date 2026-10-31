# Track D: Traverse

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/d-traverse.md).

## Approach

Stage D1, a plain and correct baseline in Rust with the standard library only.

- **Read.** Both input files are `mmap`ed and read by a hand-written byte parser that walks the digits (no `split`, no `parse`). Edges go into three `u32` arrays.
- **Build.** A forward and a reverse compressed-sparse-row graph, each made by a counting sort over the edge list. Each edge is stored as `(other end, weight)`. Parallel edges, duplicate lines, self-loops and weight-0 edges are kept as they are: relaxing them all is correct, because only the lightest can improve a distance and a self-loop never does.
- **Query.** Bidirectional Dijkstra with a binary heap, distances in `u64`. It settles from the side whose top key is smaller, folds every node both searches have reached into the best path found, and stops when the two top keys add up to at least that best path. If either side runs out of nodes the search stops too, and the answer is the best path found, or `-1` if there is none. `s == t` answers 0 at once, and a source with no out-edges or a target with no in-edges answers `-1` at once, so unreachable queries never scan the graph.
- **Parallel.** The queries are handed out in batches of 8 from one atomic counter to `available_parallelism` threads. Each thread owns its label arrays, which are reset by a per-query epoch number instead of being cleared.
- **Write.** Answers are stored by query index and written in input order, one per line.

`SD_WORKERS` (thread count) and `SD_TIMING` (phase times on stderr) are optional environment variables for local measurement; the board sets neither.

## Dependencies

None.

## With another 24 hours

Parse the file in parallel chunks and build the CSR in parallel; renumber nodes for cache locality (the input ids are a random relabelling); shrink the per-thread label arrays and use a cheaper heap; and measure the thread count on the scoring box.
