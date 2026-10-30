use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;

extern "C" {
    fn mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut u8;
}

// Map the file read-only (PROT_READ, MAP_PRIVATE) instead of reading it onto the heap.
fn map(path: &str) -> &'static [u8] {
    let file = File::open(path).unwrap();
    let len = file.metadata().unwrap().len() as usize;
    if len == 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0), len) }
}

// Nearest rank: the value at 1-indexed rank ceil(p * n / 100).
fn percentile(sorted: &[u32], p: usize) -> u32 {
    sorted[(p * sorted.len() + 99) / 100 - 1]
}

// A fast multiply-and-rotate hasher (FxHash style) in place of the default SipHash.
#[derive(Default)]
struct Fx(u64);

impl Hasher for Fx {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }
}

// (hour "YYYY-MM-DDTHH", endpoint) -> (latencies, 5xx count).
type Groups<'a> = HashMap<(&'a [u8], &'a [u8]), (Vec<u32>, u64), BuildHasherDefault<Fx>>;

fn scan(chunk: &[u8]) -> Groups<'_> {
    let mut groups = Groups::default();
    for line in chunk.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
        let mut f = line.split(|&b| b == b' ');
        let ts = f.next().unwrap();
        let endpoint = f.nth(1).unwrap();
        let status = f.next().unwrap();
        let latency = f.next().unwrap().iter().fold(0u32, |n, &d| n * 10 + (d - b'0') as u32);
        let group = groups.entry((&ts[..13], endpoint)).or_default();
        group.0.push(latency);
        group.1 += (status[0] == b'5') as u64;
    }
    groups
}

fn main() {
    let mut args = std::env::args().skip(1);
    let data = args.next().unwrap_or_else(|| "/data".into());
    let out = args.next().unwrap_or_else(|| "/out".into());
    let input = map(&format!("{data}/requests.log"));

    // Cut the file after a newline into one chunk per thread; each thread fills its own table.
    let threads = std::thread::available_parallelism().unwrap().get();
    let mut cuts = vec![0];
    for i in 1..threads {
        let from = (input.len() * i / threads).max(cuts[i - 1]);
        let line_end = input[from..].iter().position(|&b| b == b'\n').map_or(0, |n| n + 1);
        cuts.push(from + line_end);
    }
    cuts.push(input.len());
    let tables: Vec<Groups> = std::thread::scope(|s| {
        let spawned: Vec<_> = cuts.windows(2).map(|w| s.spawn(|| scan(&input[w[0]..w[1]]))).collect();
        spawned.into_iter().map(|t| t.join().unwrap()).collect()
    });
    let mut tables = tables.into_iter();
    let mut groups = tables.next().unwrap();
    for table in tables {
        for (key, (latencies, errors)) in table {
            let group = groups.entry(key).or_default();
            group.0.extend(latencies);
            group.1 += errors;
        }
    }

    // Output order: hour, then endpoint bytes, which is what tuple comparison gives.
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_unstable_by_key(|g| g.0);

    let mut text = Vec::new();
    for ((hour, endpoint), (mut latencies, errors)) in groups {
        latencies.sort_unstable();
        let count = latencies.len() as u64;
        text.extend_from_slice(hour);
        text.extend_from_slice(b":00:00Z ");
        text.extend_from_slice(endpoint);
        writeln!(
            text,
            " {count} {} {} {} {errors} {}",
            percentile(&latencies, 50),
            percentile(&latencies, 95),
            percentile(&latencies, 99),
            errors * 1_000_000 / count
        )
        .unwrap();
    }
    std::fs::write(format!("{out}/summary.txt"), text).unwrap();
}
