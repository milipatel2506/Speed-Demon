use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;

// Nearest rank: the value at 1-indexed rank ceil(p * n / 100).
fn percentile(sorted: &[u32], p: usize) -> u32 {
    sorted[(p * sorted.len() + 99) / 100 - 1]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let data = args.next().unwrap_or_else(|| "/data".into());
    let out = args.next().unwrap_or_else(|| "/out".into());
    let input = fs::read_to_string(format!("{data}/requests.log")).unwrap();

    // (hour "YYYY-MM-DDTHH", endpoint) -> (latencies, 5xx count). The BTreeMap key order
    // is hour, then endpoint bytes, which is the required output order.
    let mut groups: BTreeMap<(&str, &str), (Vec<u32>, u64)> = BTreeMap::new();
    for line in input.lines() {
        let mut f = line.split(' ');
        let ts = f.next().unwrap();
        let _method = f.next().unwrap();
        let endpoint = f.next().unwrap();
        let status: u16 = f.next().unwrap().parse().unwrap();
        let latency: u32 = f.next().unwrap().parse().unwrap();
        let group = groups.entry((&ts[..13], endpoint)).or_default();
        group.0.push(latency);
        if (500..=599).contains(&status) {
            group.1 += 1;
        }
    }

    let mut text = String::new();
    for ((hour, endpoint), (mut latencies, errors)) in groups {
        latencies.sort_unstable();
        let count = latencies.len() as u64;
        writeln!(
            text,
            "{hour}:00:00Z {endpoint} {count} {} {} {} {errors} {}",
            percentile(&latencies, 50),
            percentile(&latencies, 95),
            percentile(&latencies, 99),
            errors * 1_000_000 / count
        )
        .unwrap();
    }
    fs::write(format!("{out}/summary.txt"), text).unwrap();
}
