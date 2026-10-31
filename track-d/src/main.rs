use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

extern "C" {
    fn mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut u8;
}

const INF: u64 = u64::MAX;
const BATCH: usize = 8;

// Report a failure on `path` plainly and exit non-zero, instead of panicking.
fn fail(path: &str, error: std::io::Error) -> ! {
    eprintln!("error: {path}: {error}");
    std::process::exit(1)
}

// Map the file read-only (PROT_READ, MAP_PRIVATE) instead of reading it onto the heap.
fn map(path: &str) -> &'static [u8] {
    let file = File::open(path).unwrap_or_else(|e| fail(path, e));
    let len = file.metadata().unwrap_or_else(|e| fail(path, e)).len() as usize;
    if len == 0 {
        return &[];
    }
    // SAFETY: a null hint, a valid open fd and the file's own length; the result is checked below.
    let addr = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0) };
    if addr as isize == -1 {
        fail(path, std::io::Error::last_os_error());
    }
    // SAFETY: `addr` is a successful mapping of `len` readable bytes that is never unmapped.
    unsafe { std::slice::from_raw_parts(addr, len) }
}

// Reads the unsigned decimal numbers of a file one after another, whatever whitespace separates them.
struct Numbers {
    bytes: &'static [u8],
    at: usize,
}

impl Iterator for Numbers {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        let b = self.bytes;
        let mut i = self.at;
        while i < b.len() && !b[i].is_ascii_digit() {
            i += 1;
        }
        if i == b.len() {
            return None;
        }
        let mut x = 0;
        while i < b.len() && b[i].is_ascii_digit() {
            x = x * 10 + (b[i] - b'0') as u64;
            i += 1;
        }
        self.at = i;
        Some(x)
    }
}

// Compressed sparse rows: node u's edges are `adj[off[u]..off[u + 1]]`, each (other end, weight).
struct Csr {
    off: Vec<u32>,
    adj: Vec<(u32, u32)>,
}

impl Csr {
    // Counting sort of the edges (from[i] -> to[i], weight[i]) by `from`.
    fn build(n: usize, from: &[u32], to: &[u32], weight: &[u32]) -> Csr {
        let mut off = vec![0u32; n + 1];
        for &u in from {
            off[u as usize + 1] += 1;
        }
        for u in 0..n {
            off[u + 1] += off[u];
        }
        let mut next = off.clone();
        let mut adj = vec![(0, 0); from.len()];
        for i in 0..from.len() {
            let slot = &mut next[from[i] as usize];
            adj[*slot as usize] = (to[i], weight[i]);
            *slot += 1;
        }
        Csr { off, adj }
    }

    fn edges(&self, u: u32) -> &[(u32, u32)] {
        &self.adj[self.off[u as usize] as usize..self.off[u as usize + 1] as usize]
    }

    fn degree(&self, u: u32) -> u32 {
        self.off[u as usize + 1] - self.off[u as usize]
    }
}

// A tentative distance, valid only if `epoch` is the current query's number.
#[derive(Clone, Copy)]
struct Label {
    dist: u64,
    epoch: u32,
}

type Heap = BinaryHeap<Reverse<(u64, u32)>>;

fn label(labels: &[Label], v: u32, epoch: u32) -> u64 {
    let l = labels[v as usize];
    if l.epoch == epoch {
        l.dist
    } else {
        INF
    }
}

// One thread's search state; the label arrays are reused across queries by bumping `epoch`.
struct Search<'a> {
    fwd: &'a Csr,
    rev: &'a Csr,
    lf: Vec<Label>,
    lb: Vec<Label>,
    hf: Heap,
    hb: Heap,
    epoch: u32,
}

// Settle the top of `heap` over `graph`, relaxing its edges; `other` holds the opposite search's
// labels, and any node both searches have reached gives a path of length own + other, folded into `best`.
fn step(
    graph: &Csr,
    own: &mut [Label],
    heap: &mut Heap,
    other: &[Label],
    epoch: u32,
    best: &mut u64,
) {
    let Some(Reverse((d, u))) = heap.pop() else {
        return;
    };
    if d > own[u as usize].dist {
        return; // stale entry: u was already settled at a shorter distance
    }
    for &(v, w) in graph.edges(u) {
        let nd = d + w as u64;
        if nd < label(own, v, epoch) {
            own[v as usize] = Label { dist: nd, epoch };
            heap.push(Reverse((nd, v)));
            let far = label(other, v, epoch);
            if far != INF {
                *best = (*best).min(nd + far);
            }
        }
    }
}

impl<'a> Search<'a> {
    fn new(fwd: &'a Csr, rev: &'a Csr) -> Self {
        let n = fwd.off.len() - 1;
        let blank = Label { dist: 0, epoch: 0 };
        Search {
            fwd,
            rev,
            lf: vec![blank; n],
            lb: vec![blank; n],
            hf: Heap::new(),
            hb: Heap::new(),
            epoch: 0,
        }
    }

    // Shortest distance from s to t, or -1. Bidirectional Dijkstra: always settle from the side with
    // the smaller top key; stop when the two tops add up to at least the best path found, or when one
    // side has run dry (it has then reached everything it can).
    fn distance(&mut self, s: u32, t: u32) -> i64 {
        if s == t {
            return 0;
        }
        if self.fwd.degree(s) == 0 || self.rev.degree(t) == 0 {
            return -1;
        }
        self.epoch += 1;
        let epoch = self.epoch;
        self.hf.clear();
        self.hb.clear();
        self.lf[s as usize] = Label { dist: 0, epoch };
        self.lb[t as usize] = Label { dist: 0, epoch };
        self.hf.push(Reverse((0, s)));
        self.hb.push(Reverse((0, t)));
        let mut best = INF;
        while let (Some(&Reverse((tf, _))), Some(&Reverse((tb, _)))) =
            (self.hf.peek(), self.hb.peek())
        {
            if tf + tb >= best {
                break;
            }
            if tf <= tb {
                step(
                    self.fwd,
                    &mut self.lf,
                    &mut self.hf,
                    &self.lb,
                    epoch,
                    &mut best,
                );
            } else {
                step(
                    self.rev,
                    &mut self.lb,
                    &mut self.hb,
                    &self.lf,
                    epoch,
                    &mut best,
                );
            }
        }
        if best == INF {
            -1
        } else {
            best as i64
        }
    }
}

fn main() {
    let clock = Instant::now();
    let lap = || clock.elapsed().as_secs_f64() * 1e3;
    let graph_bytes = map("/data/graph.txt");
    let query_bytes = map("/data/queries.txt");

    let mut nums = Numbers {
        bytes: graph_bytes,
        at: 0,
    };
    let n = nums.next().unwrap_or(0) as usize;
    let m = nums.next().unwrap_or(0) as usize;
    let (mut from, mut to, mut weight) = (
        Vec::with_capacity(m),
        Vec::with_capacity(m),
        Vec::with_capacity(m),
    );
    while let (Some(u), Some(v), Some(w)) = (nums.next(), nums.next(), nums.next()) {
        from.push(u as u32);
        to.push(v as u32);
        weight.push(w as u32);
    }
    let queries: Vec<(u32, u32)> = {
        let mut q = Numbers {
            bytes: query_bytes,
            at: 0,
        };
        std::iter::from_fn(|| Some((q.next()? as u32, q.next()? as u32))).collect()
    };
    let t_parse = lap();

    let fwd = Csr::build(n, &from, &to, &weight);
    let rev = Csr::build(n, &to, &from, &weight);
    drop((from, to, weight));
    let t_build = lap();

    let workers = std::env::var("SD_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |p| p.get()));
    let answers: Vec<AtomicI64> = (0..queries.len()).map(|_| AtomicI64::new(0)).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let mut search = Search::new(&fwd, &rev);
                loop {
                    let start = next.fetch_add(BATCH, Relaxed);
                    if start >= queries.len() {
                        break;
                    }
                    for i in start..(start + BATCH).min(queries.len()) {
                        let (s, t) = queries[i];
                        answers[i].store(search.distance(s, t), Relaxed);
                    }
                }
            });
        }
    });
    let t_queries = lap();

    let mut out = Vec::with_capacity(answers.len() * 8);
    for a in &answers {
        let _ = writeln!(out, "{}", a.load(Relaxed));
    }
    std::fs::write("/out/distances.txt", &out).unwrap_or_else(|e| fail("/out/distances.txt", e));
    let t_write = lap();

    if std::env::var_os("SD_TIMING").is_some() {
        eprintln!(
            "workers {workers}: parse {:.1} ms, build {:.1} ms, queries {:.1} ms, write {:.1} ms",
            t_parse,
            t_build - t_parse,
            t_queries - t_build,
            t_write - t_queries
        );
    }
}
