use std::collections::BTreeSet;
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
    unsafe {
        std::slice::from_raw_parts(
            mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0),
            len,
        )
    }
}

const HOURS: usize = 72;

// Hour number 0..72 of a timestamp in the 72 hours from 2026-10-30T18:00Z that the spec promises,
// or None for any other timestamp.
fn hour_index(ts: &[u8]) -> Option<usize> {
    let d = |i: usize| (ts[i] - b'0') as isize;
    let hour =
        ((d(5) * 10 + d(6) - 10) * 31 + d(8) * 10 + d(9) - 30) * 24 + d(11) * 10 + d(12) - 18;
    (&ts[..4] == b"2026" && (0..HOURS as isize).contains(&hour)).then_some(hour as usize)
}

fn hour_label(hour: usize) -> String {
    let days = ["2026-10-30", "2026-10-31", "2026-11-01", "2026-11-02"];
    format!("{}T{:02}:00:00Z", days[(hour + 18) / 24], (hour + 18) % 24)
}

// FxHash-style multiply-rotate hash, folding in the bytes 8 at a time (the last word zero-padded).
fn fold(mut h: u64, bytes: &[u8]) -> u64 {
    for chunk in bytes.chunks(8) {
        let mut word = [0; 8];
        word[..chunk.len()].copy_from_slice(chunk);
        h = mix(h, u64::from_le_bytes(word));
    }
    h
}

fn mix(h: u64, word: u64) -> u64 {
    (h.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95)
}

fn hash(bytes: &[u8]) -> usize {
    (fold(0, bytes) >> 32) as usize
}

// One thread's share of the file. A group is (endpoint id, hour number), numbered id * hours + hour,
// with ids dense in order of first appearance. `lat[start[g]..start[g + 1]]` holds group g's latencies.
struct Part<'a> {
    names: Vec<&'a [u8]>,
    errors: Vec<u32>,
    start: Vec<u32>,
    lat: Vec<u32>,
}

// Scan one chunk. `hour_of` numbers a timestamp's hour in 0..hours; None abandons the scan.
fn scan<'a>(
    chunk: &'a [u8],
    hours: usize,
    hour_of: &impl Fn(&[u8]) -> Option<usize>,
) -> Option<Part<'a>> {
    let mut slots = vec![0u32; 4096]; // endpoint id + 1, or 0 for an empty slot; doubles when half full
    let mut names: Vec<&[u8]> = Vec::new();
    let (mut start, mut errors) = (Vec::new(), Vec::new()); // `start` counts requests until the sort below
    let mut rows = Vec::new(); // (group, latency) for every line, in file order
    let mut i = 0;
    while i < chunk.len() {
        // One forward pass over the line: ts SP method SP endpoint SP status SP latency LF.
        let ts = &chunk[i..i + 27];
        i += 28;
        while chunk[i] != b' ' {
            i += 1;
        }
        let from = i + 1;
        let mut h = 0;
        i = from;
        // Eight bytes at a time: a zero byte in the word xor spaces marks the end of the endpoint.
        while let Some(bytes) = chunk.get(i..i + 8) {
            let word = u64::from_le_bytes(bytes.try_into().unwrap());
            let x = word ^ 0x2020_2020_2020_2020;
            let space = x.wrapping_sub(0x0101_0101_0101_0101) & !x & 0x8080_8080_8080_8080;
            if space == 0 {
                h = mix(h, word);
                i += 8;
                continue;
            }
            let n = space.trailing_zeros() as usize / 8;
            if n > 0 {
                h = mix(h, word & (u64::MAX >> (64 - 8 * n)));
            }
            i += n;
            break;
        }
        if chunk[i] != b' ' {
            // Fewer than eight bytes left in the chunk.
            let end = i + chunk[i..].iter().position(|&b| b == b' ').unwrap();
            h = fold(h, &chunk[i..end]);
            i = end;
        }
        let endpoint = &chunk[from..i];
        let error = chunk[i + 1] == b'5';
        i += 5;
        let mut latency = 0u32;
        while i < chunk.len() && chunk[i] != b'\n' {
            latency = latency * 10 + (chunk[i] - b'0') as u32;
            i += 1;
        }
        i += 1;
        let mut slot = (h >> 32) as usize & (slots.len() - 1);
        let id = loop {
            match slots[slot] {
                0 => {
                    names.push(endpoint);
                    slots[slot] = names.len() as u32;
                    start.resize(names.len() * hours, 0);
                    errors.resize(names.len() * hours, 0);
                    if names.len() * 2 > slots.len() {
                        slots = vec![0; slots.len() * 2];
                        for (id, name) in names.iter().enumerate() {
                            let mut slot = hash(name) & (slots.len() - 1);
                            while slots[slot] != 0 {
                                slot = (slot + 1) & (slots.len() - 1);
                            }
                            slots[slot] = id as u32 + 1;
                        }
                    }
                    break names.len() - 1;
                }
                s if names[s as usize - 1] == endpoint => break s as usize - 1,
                _ => slot = (slot + 1) & (slots.len() - 1),
            }
        };
        let group = id * hours + hour_of(ts)?;
        rows.push((group, latency));
        start[group] += 1;
        errors[group] += error as u32;
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
    Some(Part {
        names,
        errors,
        start,
        lat,
    })
}

// Scan the chunks (a thread each) with one numbering of the hours; None if any line falls outside it.
fn scan_all<'a>(
    input: &'a [u8],
    cuts: &[usize],
    hours: usize,
    hour_of: impl Fn(&[u8]) -> Option<usize> + Sync,
) -> Option<Vec<Part<'a>>> {
    std::thread::scope(|s| {
        let hour_of = &hour_of;
        let spawned: Vec<_> = cuts
            .windows(2)
            .map(|w| s.spawn(move || scan(&input[w[0]..w[1]], hours, hour_of)))
            .collect();
        spawned.into_iter().map(|t| t.join().unwrap()).collect()
    })
}

// Nearest rank: the value at 1-indexed rank ceil(p * n / 100).
fn percentile(sorted: &[u32], p: usize) -> u32 {
    sorted[(p * sorted.len() + 99) / 100 - 1]
}

// The summary lines for a range of hours. `maps[t][rank]` is endpoint `rank` (in sorted order)
// as thread t's local id, if thread t saw it.
fn render(
    hours: std::ops::Range<usize>,
    labels: &[String],
    names: &[&[u8]],
    parts: &[Part],
    maps: &[Vec<Option<usize>>],
) -> Vec<u8> {
    let (mut text, mut lat) = (Vec::new(), Vec::new());
    for hour in hours {
        for (rank, name) in names.iter().enumerate() {
            lat.clear();
            let mut errors = 0;
            for (part, map) in parts.iter().zip(maps) {
                if let Some(id) = map[rank] {
                    let g = id * labels.len() + hour;
                    lat.extend_from_slice(
                        &part.lat[part.start[g] as usize..part.start[g + 1] as usize],
                    );
                    errors += part.errors[g] as u64;
                }
            }
            if lat.is_empty() {
                continue;
            }
            lat.sort_unstable();
            let count = lat.len() as u64;
            write!(text, "{} ", labels[hour]).unwrap();
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
        let line_end = input[from..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(input.len() - from, |n| n + 1);
        cuts.push(from + line_end);
    }
    cuts.push(input.len());

    // Fast path: the spec's 72-hour window. If any timestamp is outside it, number the distinct hours
    // actually present (their first 13 bytes, which sort chronologically) and scan again.
    let mut labels: Vec<String> = (0..HOURS).map(hour_label).collect();
    let parts = scan_all(input, &cuts, HOURS, hour_index).unwrap_or_else(|| {
        let hours: BTreeSet<&[u8]> = input
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| &l[..13])
            .collect();
        labels = hours
            .iter()
            .map(|h| format!("{}:00:00Z", String::from_utf8_lossy(h)))
            .collect();
        let find = |ts: &[u8]| hours.iter().position(|h| *h == &ts[..13]);
        scan_all(input, &cuts, labels.len(), find).unwrap()
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

    // Each thread renders a run of consecutive hours, so the pieces concatenate in output order.
    let hours = labels.len();
    let (labels, names, parts, maps) = (&labels, &names, &parts, &maps);
    let pieces: Vec<Vec<u8>> = std::thread::scope(|s| {
        let spawned: Vec<_> = (0..threads)
            .map(|i| {
                s.spawn(move || {
                    render(
                        hours * i / threads..hours * (i + 1) / threads,
                        labels,
                        names,
                        parts,
                        maps,
                    )
                })
            })
            .collect();
        spawned.into_iter().map(|t| t.join().unwrap()).collect()
    });
    let mut file = File::create(format!("{out}/summary.txt")).unwrap();
    for piece in pieces {
        file.write_all(&piece).unwrap();
    }
}
