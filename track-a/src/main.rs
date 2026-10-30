use std::collections::HashMap;
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

fn main() {
    let mut args = std::env::args().skip(1);
    let data = args.next().unwrap_or_else(|| "/data".into());
    let out = args.next().unwrap_or_else(|| "/out".into());
    let input = map(&format!("{data}/requests.log"));

    // (hour "YYYY-MM-DDTHH", endpoint) -> (latencies, 5xx count).
    let mut groups: HashMap<(&[u8], &[u8]), (Vec<u32>, u64)> = HashMap::new();
    for line in input.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
        let mut f = line.split(|&b| b == b' ');
        let ts = f.next().unwrap();
        let endpoint = f.nth(1).unwrap();
        let status = f.next().unwrap();
        let latency = f.next().unwrap().iter().fold(0u32, |n, &d| n * 10 + (d - b'0') as u32);
        let group = groups.entry((&ts[..13], endpoint)).or_default();
        group.0.push(latency);
        group.1 += (status[0] == b'5') as u64;
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
