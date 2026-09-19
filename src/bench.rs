//! The measurement driver: one dataset × one codec → one [`Record`].
//!
//! What is measured and why:
//!
//! * **density** — bits/int over the whole dataset, and the same minus the
//!   uniform-model entropy bound ([`crate::stream::entropy_bits`]). Negative
//!   excess means the codec exploited clustering; positive is overhead.
//! * **encode** — bulk encode of every list, Mi/s. This is segment write and
//!   the re-encode half of a merge.
//! * **decode (arena)** — decode every list in order from one arena the size
//!   of the whole encoded dataset, so a dataset bigger than L3 measures the
//!   DRAM-resident case and a small one the cache-resident case. The arena
//!   size is recorded so the reader knows which.
//! * **decode (hot)** — one representative list decoded repeatedly, L1-hot.
//!   The codec's raw kernel speed, with memory taken out.
//! * **open** — preparing every list for queries (`Codec::open`), ns per
//!   list. Zero for in-place formats; the deserialisation cost for owning
//!   structures, which an index would pay once per segment open, not per
//!   query.
//! * **intersect** — leapfrog AND of list pairs at length ratios 1:1, 1:10,
//!   1:100, 1:1000 via the codec's own `next_geq`, ns per element of the
//!   shorter list. This is where skip structures earn their bytes or don't.
//! * **seek** — a fresh cursor and one `next_geq` to a random target in a
//!   long list: the cost of a single skip from cold.
//! * **get** — random access by ordinal, where the codec supports it.
//! * **encode/decode peak memory** — peak-RSS delta while encoding, then
//!   decoding, the single longest list. Coarse, but it separates streaming
//!   codecs from ones that need the whole list (or a DP table over it).

use std::time::Duration;

use rand::prelude::*;
use serde::{Deserialize, Serialize};

use crate::codec::{self, Codec, Prepared};
use crate::stream::{Dataset, Kind, entropy_bits};
use crate::timer::{self, Sample};

#[derive(Clone, Debug)]
pub struct Config {
    pub seed: u64,
    pub min_runs: u32,
    pub max_runs: u32,
    pub budget: Duration,
    /// Cap on lists conformance-checked before timing (all lists if fewer).
    pub check_lists: usize,
    pub probes: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self { seed: 42, min_runs: 7, max_runs: 100, budget: Duration::from_secs(2), check_lists: 2000, probes: 20_000 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub dataset: String,
    pub kind: Kind,
    pub codec: String,
    pub lists: usize,
    pub ints: usize,
    /// `None` when the codec failed conformance; `error` says how.
    pub error: Option<String>,
    pub streaming_encoder: bool,
    pub bytes: usize,
    pub bits_per_int: f64,
    pub entropy_bits_per_int: f64,
    pub excess_bits_per_int: f64,
    pub aux_bits_per_int: Option<f64>,
    pub encode: Option<Sample>,
    pub decode_arena: Option<Sample>,
    pub decode_hot: Option<Sample>,
    pub decode_hot_ints: usize,
    /// Keyed by ratio (1, 10, 100, 1000); each is total ns over the pairs
    /// with `pairs_short_ints` elements in the short lists, so per-element
    /// cost is `ns / ints`.
    pub open: Option<Sample>,
    pub intersect: Vec<Intersect>,
    pub seek: Option<Sample>,
    pub seek_probes: usize,
    pub get: Option<Sample>,
    pub get_probes: usize,
    pub encode_peak_kb: Option<u64>,
    pub decode_peak_kb: Option<u64>,
    pub longest_list: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Intersect {
    pub ratio: u32,
    pub pairs: usize,
    pub short_ints: usize,
    pub long_ints: usize,
    pub sample: Sample,
}

struct Arena {
    bytes: Vec<u8>,
    /// `(offset, len)` per list.
    spans: Vec<(usize, usize)>,
}

fn encode_all(codec: &dyn Codec, ds: &Dataset, arena: &mut Arena) {
    arena.bytes.clear();
    arena.spans.clear();
    for list in &ds.lists {
        let start = arena.bytes.len();
        codec.encode(ds.meta.kind, ds.meta.universe, list, &mut arena.bytes);
        arena.spans.push((start, arena.bytes.len() - start));
    }
}

pub fn run(codec: &dyn Codec, ds: &Dataset, cfg: &Config) -> Record {
    let kind = ds.meta.kind;
    let universe = ds.meta.universe;
    let ints = ds.total_ints();
    let mut rec = Record {
        dataset: ds.meta.name.clone(),
        kind,
        codec: codec.name().to_string(),
        lists: ds.lists.len(),
        ints,
        error: None,
        streaming_encoder: codec.caps().streaming_encoder,
        bytes: 0,
        bits_per_int: 0.0,
        entropy_bits_per_int: 0.0,
        excess_bits_per_int: 0.0,
        aux_bits_per_int: None,
        encode: None,
        decode_arena: None,
        decode_hot: None,
        decode_hot_ints: 0,
        open: None,
        intersect: Vec::new(),
        seek: None,
        seek_probes: 0,
        get: None,
        get_probes: 0,
        encode_peak_kb: None,
        decode_peak_kb: None,
        longest_list: ds.lists.iter().map(Vec::len).max().unwrap_or(0),
    };
    if kind == Kind::Unsorted && codec.caps().sorted_only {
        rec.error = Some("sorted-only codec".into());
        return rec;
    }
    if ints == 0 {
        rec.error = Some("empty dataset".into());
        return rec;
    }
    let mut rng = StdRng::seed_from_u64(cfg.seed);

    // ── conformance ──
    let check_idx: Vec<usize> = if ds.lists.len() <= cfg.check_lists {
        (0..ds.lists.len()).collect()
    } else {
        let mut idx: Vec<usize> = (0..cfg.check_lists / 2).collect();
        idx.extend((0..cfg.check_lists / 2).map(|_| rng.random_range(0..ds.lists.len())));
        idx
    };
    for &i in &check_idx {
        if let Err(e) = codec::check(codec, kind, universe, &ds.lists[i]) {
            rec.error = Some(format!("list {i}: {e}"));
            return rec;
        }
    }

    // ── encode + density ──
    let mut arena = Arena { bytes: Vec::new(), spans: Vec::new() };
    rec.encode = Some(timer::measure(cfg.min_runs, cfg.max_runs, cfg.budget, || encode_all(codec, ds, &mut arena)));
    rec.bytes = arena.bytes.len();
    rec.bits_per_int = rec.bytes as f64 * 8.0 / ints as f64;
    let entropy: f64 = ds.lists.iter().map(|l| entropy_bits(kind, universe, l)).sum();
    rec.entropy_bits_per_int = entropy / ints as f64;
    rec.excess_bits_per_int = rec.bits_per_int - rec.entropy_bits_per_int;
    let aux: Option<usize> =
        ds.lists.iter().zip(&arena.spans).map(|(l, &(o, n))| codec.aux_bytes(l.len(), &arena.bytes[o..o + n])).sum();
    rec.aux_bits_per_int = aux.map(|a| a as f64 * 8.0 / ints as f64);

    // ── decode, arena order ──
    let mut out = Vec::with_capacity(rec.longest_list);
    rec.decode_arena = Some(timer::measure(cfg.min_runs, cfg.max_runs, cfg.budget, || {
        for (list, &(o, n)) in ds.lists.iter().zip(&arena.spans) {
            out.clear();
            codec.decode(kind, universe, list.len(), &arena.bytes[o..o + n], &mut out);
            std::hint::black_box(&out);
        }
    }));

    // ── decode, hot ──
    // The longest list up to 64K ints: long enough to amortise per-list
    // setup, short enough to stay in L1/L2 with its output.
    let hot =
        ds.lists.iter().enumerate().filter(|(_, l)| l.len() <= 65_536).max_by_key(|(_, l)| l.len()).map(|(i, _)| i);
    if let Some(i) = hot {
        let (o, n) = arena.spans[i];
        let len = ds.lists[i].len();
        let reps = (1 << 20) / len.max(1);
        rec.decode_hot_ints = len * reps;
        rec.decode_hot = Some(timer::measure(cfg.min_runs, cfg.max_runs, cfg.budget, || {
            for _ in 0..reps {
                out.clear();
                codec.decode(kind, universe, len, &arena.bytes[o..o + n], &mut out);
                std::hint::black_box(&out);
            }
        }));
    }

    // ── open ──
    let needs_open = (kind == Kind::Sorted && codec.caps().seek) || codec.caps().random_access;
    let mut prepared: Vec<Box<dyn Prepared>> = Vec::new();
    if needs_open {
        rec.open = Some(timer::measure(cfg.min_runs, cfg.max_runs, cfg.budget, || {
            prepared.clear();
            prepared.extend(
                ds.lists
                    .iter()
                    .zip(&arena.spans)
                    .map(|(l, &(o, n))| codec::prepare(codec, kind, universe, l.len(), &arena.bytes[o..o + n])),
            );
        }));
    }

    // ── seek / intersect (sorted, with cursor) ──
    if kind == Kind::Sorted && codec.caps().seek {
        for ratio in [1u32, 10, 100, 1000] {
            if let Some(ix) = intersect_bench(codec, ds, &prepared, ratio, cfg, &mut rng) {
                rec.intersect.push(ix);
            }
        }
        let long: Vec<usize> = (0..ds.lists.len()).filter(|&i| ds.lists[i].len() >= 1024).collect();
        if !long.is_empty() {
            let probes: Vec<(usize, u32)> = (0..cfg.probes)
                .map(|_| (long[rng.random_range(0..long.len())], rng.random_range(0..universe)))
                .collect();
            rec.seek_probes = probes.len();
            rec.seek = Some(timer::measure(cfg.min_runs, cfg.max_runs, cfg.budget, || {
                for &(i, target) in &probes {
                    if let Some(mut cur) = prepared[i].cursor() {
                        std::hint::black_box(cur.next_geq(target));
                    }
                }
            }));
        }
    }

    // ── random access ──
    if codec.caps().random_access {
        let nonempty: Vec<usize> = (0..ds.lists.len()).filter(|&i| !ds.lists[i].is_empty()).collect();
        let probes: Vec<(usize, usize)> = (0..cfg.probes)
            .map(|_| {
                let i = nonempty[rng.random_range(0..nonempty.len())];
                (i, rng.random_range(0..ds.lists[i].len()))
            })
            .collect();
        rec.get_probes = probes.len();
        rec.get = Some(timer::measure(cfg.min_runs, cfg.max_runs, cfg.budget, || {
            for &(i, j) in &probes {
                std::hint::black_box(prepared[i].get(j));
            }
        }));
    }

    // ── peak memory on the longest list ──
    if let Some(longest) = ds.lists.iter().max_by_key(|l| l.len()) {
        let mut buf = Vec::new();
        if timer::reset_peak_rss() {
            let base = timer::rss_kb();
            codec.encode(kind, universe, longest, &mut buf);
            rec.encode_peak_kb = timer::peak_rss_kb().zip(base).map(|(p, b)| p.saturating_sub(b));
            let mut out = Vec::new();
            if timer::reset_peak_rss() {
                let base = timer::rss_kb();
                codec.decode(kind, universe, longest.len(), &buf, &mut out);
                rec.decode_peak_kb = timer::peak_rss_kb().zip(base).map(|(p, b)| p.saturating_sub(b));
            }
            std::hint::black_box(&out);
        }
    }
    rec
}

/// Pair lists whose length ratio is about `ratio`, leapfrog-intersect them
/// with the codec's cursors, and check the count against a naive merge on
/// the first pair.
fn intersect_bench(
    codec: &dyn Codec,
    ds: &Dataset,
    prepared: &[Box<dyn Prepared + '_>],
    ratio: u32,
    cfg: &Config,
    rng: &mut StdRng,
) -> Option<Intersect> {
    // Candidate short lists: at least 32 elements, and a partner exists.
    let mut by_len: Vec<usize> = (0..ds.lists.len()).filter(|&i| ds.lists[i].len() >= 32).collect();
    by_len.sort_by_key(|&i| ds.lists[i].len());
    if by_len.len() < 2 {
        return None;
    }
    let lens: Vec<usize> = by_len.iter().map(|&i| ds.lists[i].len()).collect();
    let mut pairs = Vec::new();
    let mut attempts = 0;
    while pairs.len() < 64 && attempts < 4096 {
        attempts += 1;
        let s = rng.random_range(0..by_len.len());
        let want = lens[s] * ratio as usize;
        // Nearest list by length to the wanted partner length.
        let p = lens.partition_point(|&l| l < want);
        let cands = [p.saturating_sub(1), p.min(lens.len() - 1)];
        let l = *cands.iter().min_by_key(|&&c| lens[c].abs_diff(want))?;
        if l == s {
            continue;
        }
        let actual = lens[l] as f64 / lens[s] as f64;
        if actual < ratio as f64 * 0.5 || actual > ratio as f64 * 2.0 {
            continue;
        }
        pairs.push((by_len[s], by_len[l]));
    }
    if pairs.is_empty() {
        return None;
    }
    let short_ints: usize = pairs.iter().map(|&(s, _)| ds.lists[s].len()).sum();
    let long_ints: usize = pairs.iter().map(|&(_, l)| ds.lists[l].len()).sum();

    let leapfrog = |s: usize, l: usize| -> usize {
        let (Some(mut a), Some(mut b)) = (prepared[s].cursor(), prepared[l].cursor()) else {
            return 0;
        };
        let mut count = 0;
        let Some(mut x) = a.next() else { return 0 };
        while let Some(y) = b.next_geq(x) {
            if y == x {
                count += 1;
                let Some(nx) = a.next() else { break };
                x = nx;
            } else {
                let Some(nx) = a.next_geq(y) else { break };
                x = nx;
            }
        }
        count
    };
    // Correctness on the first pair.
    let (s, l) = pairs[0];
    let naive = naive_intersect_count(&ds.lists[s], &ds.lists[l]);
    let got = leapfrog(s, l);
    assert_eq!(got, naive, "{}: intersect count mismatch on lists {s}×{l}", codec.name());

    let sample = timer::measure(cfg.min_runs, cfg.max_runs, cfg.budget, || {
        for &(s, l) in &pairs {
            std::hint::black_box(leapfrog(s, l));
        }
    });
    Some(Intersect { ratio, pairs: pairs.len(), short_ints, long_ints, sample })
}

fn naive_intersect_count(a: &[u32], b: &[u32]) -> usize {
    let (mut i, mut j, mut n) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                n += 1;
                i += 1;
                j += 1;
            }
        }
    }
    n
}

// ── machine description for the results header ──

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Machine {
    pub hostname: String,
    pub cpu: String,
    pub governor: Option<String>,
    pub rustc: String,
    pub target_cpu_native: bool,
    pub cycles_counter: bool,
    pub os: String,
}

pub fn machine() -> Machine {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu = cpuinfo
        .lines()
        .find_map(|l| l.strip_prefix("model name"))
        .map(|s| s.trim_start_matches([' ', '\t', ':']).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    Machine {
        hostname: std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()).unwrap_or_default(),
        cpu,
        governor: std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
            .ok()
            .map(|s| s.trim().to_string()),
        rustc: option_env!("IPB_RUSTC").unwrap_or("unknown").to_string(),
        target_cpu_native: cfg!(target_feature = "avx2"),
        cycles_counter: timer::cycles_available(),
        os: std::fs::read_to_string("/proc/sys/kernel/osrelease").map(|s| s.trim().to_string()).unwrap_or_default(),
    }
}
