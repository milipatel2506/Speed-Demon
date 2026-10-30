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

const HOURS: usize = 72; // every timestamp is in the 72 hours from 2026-10-30T18:00Z (spec, section 2)

// Hour number 0..72 of a timestamp such as 2026-10-30T18:00:00.000000Z.
fn hour_index(ts: &[u8]) -> usize {
    let d = |i: usize| (ts[i] - b'0') as usize;
    ((d(5) * 10 + d(6) - 10) * 31 + d(8) * 10 + d(9) - 30) * 24 + d(11) * 10 + d(12) - 18
}

fn hour_text(hour: usize) -> String {
    let days = ["2026-10-30", "2026-10-31", "2026-11-01", "2026-11-02"];
    format!("{}T{:02}:00:00Z", days[(hour + 18) / 24], (hour + 18) % 24)
}

// FxHash-style multiply-rotate hash of the bytes, reduced to a slot of the 4096-slot endpoint table.
fn slot_of(bytes: &[u8]) -> usize {
    let mut h = 0u64;
    for chunk in bytes.chunks(8) {
        let mut word = [0; 8];
        word[..chunk.len()].copy_from_slice(chunk);
        h = (h.rotate_left(5) ^ u64::from_le_bytes(word)).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    (h >> 52) as usize
}

// One thread's share of the file. A group is (endpoint id, hour), numbered id * HOURS + hour,
// with ids dense in order of first appearance. `lat[start[g]..start[g + 1]]` holds group g's latencies.
struct Part<'a> {
    names: Vec<&'a [u8]>,
    errors: Vec<u32>,
    start: Vec<u32>,
    lat: Vec<u32>,
}

fn scan(chunk: &[u8]) -> Part<'_> {
    let mut slots = [0u16; 4096]; // endpoint id + 1, or 0 for an empty slot
    let mut names: Vec<&[u8]> = Vec::new();
    let (mut start, mut errors) = (Vec::new(), Vec::new()); // `start` counts requests until the sort below
    let mut rows = Vec::new(); // (group, latency) for every line, in file order
    for line in chunk.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
        let mut f = line.split(|&b| b == b' ');
        let ts = f.next().unwrap();
        let endpoint = f.nth(1).unwrap();
        let status = f.next().unwrap();
        let latency = f.next().unwrap().iter().fold(0u32, |n, &d| n * 10 + (d - b'0') as u32);
        let mut slot = slot_of(endpoint);
        let id = loop {
            match slots[slot] {
                0 => {
                    names.push(endpoint);
                    slots[slot] = names.len() as u16;
                    start.resize(names.len() * HOURS, 0);
                    errors.resize(names.len() * HOURS, 0);
                    break names.len() - 1;
                }
                s if names[s as usize - 1] == endpoint => break s as usize - 1,
                _ => slot = (slot + 1) % 4096,
            }
        };
        let group = id * HOURS + hour_index(ts);
        rows.push((group, latency));
        start[group] += 1;
        errors[group] += (status[0] == b'5') as u32;
    }
    // Counting sort of the latencies by group: turn the counts into start offsets, then place.
    let mut sum = 0;
    for s in &mut start {
        sum += std::mem::replace(s, sum);
    }
    start.push(sum);
    let mut next = start.clone();
    let mut lat = vec![0; rows.len()];
    for (group, latency) in rows {
        lat[next[group] as usize] = latency;
        next[group] += 1;
    }
    Part { names, errors, start, lat }
}

// Nearest rank: the value at 1-indexed rank ceil(p * n / 100).
fn percentile(sorted: &[u32], p: usize) -> u32 {
    sorted[(p * sorted.len() + 99) / 100 - 1]
}

// The summary lines for a range of hours. `maps[t][rank]` is endpoint `rank` (in sorted order)
// as thread t's local id, if thread t saw it.
fn render(hours: std::ops::Range<usize>, names: &[&[u8]], parts: &[Part], maps: &[Vec<Option<usize>>]) -> Vec<u8> {
    let (mut text, mut lat) = (Vec::new(), Vec::new());
    for hour in hours {
        for (rank, name) in names.iter().enumerate() {
            lat.clear();
            let mut errors = 0;
            for (part, map) in parts.iter().zip(maps) {
                if let Some(id) = map[rank] {
                    let g = id * HOURS + hour;
                    lat.extend_from_slice(&part.lat[part.start[g] as usize..part.start[g + 1] as usize]);
                    errors += part.errors[g] as u64;
                }
            }
            if lat.is_empty() {
                continue;
            }
            lat.sort_unstable();
            let count = lat.len() as u64;
            write!(text, "{} ", hour_text(hour)).unwrap();
            text.extend_from_slice(name);
            writeln!(
                text,
                " {count} {} {} {} {errors} {}",
                percentile(&lat, 50),
                percentile(&lat, 95),
                percentile(&lat, 99),
                errors * 1_000_000 / count
            )
            .unwrap();
        }
    }
    text
}

fn main() {
    let mut args = std::env::args().skip(1);
    let data = args.next().unwrap_or_else(|| "/data".into());
    let out = args.next().unwrap_or_else(|| "/out".into());
    let input = map(&format!("{data}/requests.log"));

    // Cut the file after a newline into one chunk per thread; each thread scans its own chunk.
    let threads = std::thread::available_parallelism().unwrap().get();
    let mut cuts = vec![0];
    for i in 1..threads {
        let from = (input.len() * i / threads).max(cuts[i - 1]);
        let line_end = input[from..].iter().position(|&b| b == b'\n').map_or(0, |n| n + 1);
        cuts.push(from + line_end);
    }
    cuts.push(input.len());
    let parts: Vec<Part> = std::thread::scope(|s| {
        let spawned: Vec<_> = cuts.windows(2).map(|w| s.spawn(|| scan(&input[w[0]..w[1]]))).collect();
        spawned.into_iter().map(|t| t.join().unwrap()).collect()
    });

    // All endpoints in output order (bytewise), and each thread's local id for each of them.
    let mut names: Vec<&[u8]> = parts.iter().flat_map(|p| p.names.iter().copied()).collect();
    names.sort_unstable();
    names.dedup();
    let maps: Vec<Vec<Option<usize>>> = parts
        .iter()
        .map(|p| {
            let mut map = vec![None; names.len()];
            for (id, name) in p.names.iter().enumerate() {
                map[names.binary_search(name).unwrap()] = Some(id);
            }
            map
        })
        .collect();

    let text = render(0..HOURS, &names, &parts, &maps);
    std::fs::write(format!("{out}/summary.txt"), text).unwrap();
}
