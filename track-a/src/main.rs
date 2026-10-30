use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::FileExt;

const HOURS: usize = 72;
const BLOCK: usize = 1 << 16;
const READ: usize = 64 << 10; // bytes per pread

// Hour number 0..72 of a timestamp in the 72 hours from 2026-10-30T18:00Z that the spec promises,
// or None for any other timestamp.
fn hour_index(ts: &[u8]) -> Option<usize> {
    let d = |i: usize| (ts[i] - b'0') as isize;
    let hour =
        ((d(5) * 10 + d(6) - 10) * 31 + d(8) * 10 + d(9) - 30) * 24 + d(11) * 10 + d(12) - 18;
    (&ts[..4] == b"2026" && (0..HOURS as isize).contains(&hour)).then_some(hour as usize)
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

// One thread's share of the file. A group is (endpoint id, hour number), numbered id * hours + hour,
// with ids dense in order of first appearance. `lat[start[g]..start[g + 1]]` holds group g's latencies.
struct Part {
    names: Vec<Vec<u8>>,
    errors: Vec<u32>,
    start: Vec<u32>,
    lat: Vec<u32>,
}

// Scan a byte range of the file (whole lines), reading it in blocks of READ bytes. `hour_of` numbers a
// timestamp's hour in 0..hours; None abandons the scan.
fn scan(
    file: &File,
    range: std::ops::Range<usize>,
    hours: usize,
    hour_of: &impl Fn(&[u8]) -> Option<usize>,
) -> Option<Part> {
    let mut slots = vec![0u32; 4096]; // endpoint id + 1, or 0 for an empty slot; doubles when half full
    let mut names: Vec<Vec<u8>> = Vec::new();
    let (mut start, mut errors) = (Vec::new(), Vec::new()); // `start` counts requests until the sort below
    let mut rows: Vec<Vec<u64>> = Vec::new(); // group << 32 | latency per line, in blocks freed as the sort consumes them
    let (mut pos, mut carry, mut buf) = (range.start, 0, vec![0; READ]);
    while pos < range.end || carry > 0 {
        // Fill the buffer after the partial line carried over, then scan up to the last whole line.
        let n = (READ - carry).min(range.end - pos);
        file.read_exact_at(&mut buf[carry..carry + n], pos as u64)
            .unwrap();
        pos += n;
        let filled = carry + n;
        let end = if pos == range.end {
            filled
        } else {
            buf[..filled].iter().rposition(|&b| b == b'\n').unwrap() + 1
        };
        let chunk = &buf[..end];
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
                        names.push(endpoint.to_vec());
                        slots[slot] = names.len() as u32;
                        start.resize(names.len() * hours, 0);
                        errors.resize(names.len() * hours, 0);
                        if names.len() * 2 > slots.len() {
                            slots = vec![0; slots.len() * 2];
                            for (id, name) in names.iter().enumerate() {
                                let mut slot = (fold(0, name) >> 32) as usize & (slots.len() - 1);
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
            if rows.last().map_or(true, |block| block.len() == BLOCK) {
                rows.push(Vec::with_capacity(BLOCK));
            }
            rows.last_mut()
                .unwrap()
                .push((group as u64) << 32 | latency as u64);
            start[group] += 1;
            errors[group] += error as u32;
        }
        buf.copy_within(end..filled, 0);
        carry = filled - end;
    }
    // Counting sort of the latencies by group: turn the counts into end offsets, then place each
    // latency just below its group's end, which leaves every entry at the group's start offset.
    let mut sum = 0;
    for s in &mut start {
        sum += *s;
        *s = sum;
    }
    let mut lat = vec![0; rows.iter().map(Vec::len).sum()];
    for row in rows.into_iter().flatten() {
        let group = (row >> 32) as usize;
        start[group] -= 1;
        lat[start[group] as usize] = row as u32;
    }
    start.push(sum);
    Some(Part {
        names,
        errors,
        start,
        lat,
    })
}

// Scan the byte ranges (a thread each) with one numbering of the hours; None if any line falls outside it.
fn scan_all(
    file: &File,
    cuts: &[usize],
    hours: usize,
    hour_of: impl Fn(&[u8]) -> Option<usize> + Sync,
) -> Option<Vec<Part>> {
    std::thread::scope(|s| {
        let hour_of = &hour_of;
        let spawned: Vec<_> = cuts
            .windows(2)
            .map(|w| s.spawn(move || scan(file, w[0]..w[1], hours, hour_of)))
            .collect();
        spawned.into_iter().map(|t| t.join().unwrap()).collect()
    })
}

fn main() {
    let mut args = std::env::args().skip(1);
    let data = args.next().unwrap_or_else(|| "/data".into());
    let out = args.next().unwrap_or_else(|| "/out".into());
    let path = format!("{data}/requests.log");
    let file = File::open(&path).unwrap();
    let len = file.metadata().unwrap().len() as usize;

    // Cut the file after a newline into one range per thread (a line is far shorter than 4096 bytes).
    let threads = std::thread::available_parallelism().unwrap().get();
    let cuts: Vec<usize> = (0..=threads)
        .map(|i| {
            let at = len * i / threads;
            let mut block = [0; 4096];
            let n = file.read_at(&mut block, at as u64).unwrap();
            let k = block[..n].iter().position(|&b| b == b'\n');
            if i == 0 {
                0
            } else {
                at + k.map_or(n, |k| k + 1)
            }
        })
        .collect();

    // Fast path: the spec's 72-hour window. If any timestamp is outside it, number the distinct hours
    // actually present (their first 13 bytes, which sort chronologically) and scan again.
    let days = ["2026-10-30", "2026-10-31", "2026-11-01", "2026-11-02"];
    let mut labels: Vec<String> = (18..18 + HOURS)
        .map(|h| format!("{}T{:02}:00:00Z", days[h / 24], h % 24))
        .collect();
    let parts = scan_all(&file, &cuts, HOURS, hour_index).unwrap_or_else(|| {
        let all = std::fs::read(&path).unwrap();
        let hours: BTreeSet<&[u8]> = all
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| &l[..13])
            .collect();
        labels = hours
            .iter()
            .map(|h| format!("{}:00:00Z", String::from_utf8_lossy(h)))
            .collect();
        let find = |ts: &[u8]| hours.iter().position(|h| *h == &ts[..13]);
        scan_all(&file, &cuts, labels.len(), find).unwrap()
    });

    // All endpoints in output order (bytewise), and each thread's local id for each of them.
    let mut names: Vec<&[u8]> = parts
        .iter()
        .flat_map(|p| p.names.iter().map(|n| n.as_slice()))
        .collect();
    names.sort_unstable();
    names.dedup();
    let maps: Vec<Vec<Option<usize>>> = parts
        .iter()
        .map(|p| {
            let mut map = vec![None; names.len()];
            for (id, name) in p.names.iter().enumerate() {
                map[names.binary_search(&name.as_slice()).unwrap()] = Some(id);
            }
            map
        })
        .collect();

    // Each thread renders a run of consecutive hours, so the pieces concatenate in output order.
    let hours = labels.len();
    let render = |range: std::ops::Range<usize>| {
        let (mut text, mut lat) = (Vec::new(), Vec::new());
        for hour in range {
            for (rank, name) in names.iter().enumerate() {
                lat.clear();
                let mut errors = 0;
                for (part, map) in parts.iter().zip(&maps) {
                    if let Some(id) = map[rank] {
                        let g = id * hours + hour;
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
                // Nearest rank: the value at 1-indexed rank ceil(p * n / 100).
                let [p50, p95, p99] = [50, 95, 99].map(|p| lat[(p * lat.len() + 99) / 100 - 1]);
                writeln!(
                    text,
                    " {count} {p50} {p95} {p99} {errors} {}",
                    errors * 1_000_000 / count
                )
                .unwrap();
            }
        }
        text
    };
    let render = &render;
    let pieces: Vec<Vec<u8>> = std::thread::scope(|s| {
        let spawned: Vec<_> = (0..threads)
            .map(|i| s.spawn(move || render(hours * i / threads..hours * (i + 1) / threads)))
            .collect();
        spawned.into_iter().map(|t| t.join().unwrap()).collect()
    });
    let mut file = File::create(format!("{out}/summary.txt")).unwrap();
    for piece in pieces {
        file.write_all(&piece).unwrap();
    }
}
