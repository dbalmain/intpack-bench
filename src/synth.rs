//! Synthetic stream generators.
//!
//! Each [`Source`] is a small parametric model of one kind of structure a
//! real index stream has. The presets in [`presets`] sweep the two parameters
//! that dominate codec behaviour on sorted lists — density (`n/universe`) and
//! burstiness — plus a Zipfian dictionary whose list-length distribution
//! decides how much the short-list tail path matters.
//!
//! Generators are deterministic for a given seed.

use std::collections::HashSet;

use rand::prelude::*;
use rand_distr::{Distribution, Geometric, Zipf};

use crate::stream::{Dataset, Kind};

// ── source ──

/// A parametric stream model. Sorted variants produce strictly increasing
/// lists in `[0, universe)`; unsorted ones produce arbitrary values.
#[derive(Clone, Debug)]
pub enum Source {
    /// Each position in the universe is present independently with
    /// probability `density`. The uniform-model entropy bound is tight here,
    /// so excess bits/int is pure codec overhead.
    Uniform { universe: u32, lists: u32, density: f64 },
    /// Two-state Markov-modulated Bernoulli ("sparse with dense patches").
    ///
    /// Let `d = density` and `L = burst_len` (mean On sojourn, in positions).
    /// Switching probabilities are `P(On→Off) = 1/L` and
    /// `P(Off→On) = d / (L (1 − d))`, so the stationary mass on On is
    /// `π_On = d`. Presence probabilities are `p_On = 1 − (1 − d)/L` and
    /// `p_Off = d/L`, which satisfy `π_On p_On + π_Off p_Off = d`. As `L → 1`
    /// this approaches Uniform; as `L → ∞` it approaches Runs.
    Clustered { universe: u32, lists: u32, density: f64, burst_len: f64 },
    /// Values at `k * stride + noise` with `noise` uniform in
    /// `[-jitter, +jitter]`, kept strictly increasing (a candidate is skipped
    /// if it would not exceed the previous value). `jitter = 0` is the
    /// pathological equal-gap case.
    Periodic { universe: u32, lists: u32, stride: u32, jitter: u32 },
    /// Solid runs of consecutive integers of mean length `run_len` (geometric),
    /// separated by geometric gaps chosen so the expected density is `density`.
    Runs { universe: u32, lists: u32, density: f64, run_len: f64 },
    /// `lists` lists whose lengths follow Zipf's law: rank `r` has length
    /// `∝ 1/r^exponent`, scaled so rank 1 has density `max_density`. Each list
    /// is drawn from [`Source::Clustered`] with `burst_len`, or
    /// [`Source::Uniform`] when `burst_len <= 0`. Every list has at least one
    /// element.
    ZipfDictionary { universe: u32, lists: u32, exponent: f64, max_density: f64, burst_len: f64 },
    /// Term-frequency-like: `1 + Geometric(p)` values, each `≥ 1`. Universe is
    /// `u32::MAX`.
    Geometric { lists: u32, len: u32, p: f64 },
    /// Uniform in `[0, 2^bits)`; the FOR ideal case. Universe is `2^bits`.
    UniformBits { lists: u32, len: u32, bits: u32 },
    /// Values in `[0, n)` with Zipf probability (hot head at 0). Models
    /// position deltas and doc-value columns.
    ZipfValues { lists: u32, len: u32, n: u32, exponent: f64 },
}

impl Source {
    pub fn kind(&self) -> Kind {
        match self {
            Source::Uniform { .. }
            | Source::Clustered { .. }
            | Source::Periodic { .. }
            | Source::Runs { .. }
            | Source::ZipfDictionary { .. } => Kind::Sorted,
            Source::Geometric { .. } | Source::UniformBits { .. } | Source::ZipfValues { .. } => Kind::Unsorted,
        }
    }

    pub fn universe(&self) -> u32 {
        match self {
            Source::Uniform { universe, .. }
            | Source::Clustered { universe, .. }
            | Source::Periodic { universe, .. }
            | Source::Runs { universe, .. }
            | Source::ZipfDictionary { universe, .. } => *universe,
            Source::Geometric { .. } => u32::MAX,
            Source::UniformBits { bits, .. } => bits_universe(*bits),
            Source::ZipfValues { n, .. } => (*n).max(1),
        }
    }

    /// One-line description for the dataset header and report.
    pub fn describe(&self) -> String {
        match self {
            Source::Uniform { universe, lists, density } => {
                format!("uniform universe={universe} lists={lists} density={density}")
            }
            Source::Clustered { universe, lists, density, burst_len } => {
                format!("clustered universe={universe} lists={lists} density={density} burst_len={burst_len}")
            }
            Source::Periodic { universe, lists, stride, jitter } => {
                format!("periodic universe={universe} lists={lists} stride={stride} jitter={jitter}")
            }
            Source::Runs { universe, lists, density, run_len } => {
                format!("runs universe={universe} lists={lists} density={density} run_len={run_len}")
            }
            Source::ZipfDictionary { universe, lists, exponent, max_density, burst_len } => {
                format!(
                    "zipf-dict universe={universe} lists={lists} exponent={exponent} max_density={max_density} burst_len={burst_len}"
                )
            }
            Source::Geometric { lists, len, p } => {
                format!("geometric lists={lists} len={len} p={p}")
            }
            Source::UniformBits { lists, len, bits } => {
                format!("uniform-bits lists={lists} len={len} bits={bits}")
            }
            Source::ZipfValues { lists, len, n, exponent } => {
                format!("zipf-values lists={lists} len={len} n={n} exponent={exponent}")
            }
        }
    }
}

// ── presets ──

/// A named preset for the `gen` command.
#[derive(Clone, Debug)]
pub struct Preset {
    pub name: &'static str,
    pub source: Source,
}

/// The default sweep written by `gen`. `scale` multiplies sorted `universe`
/// and unsorted per-list `len` so a quick run and a full run share the same
/// shapes. List counts stay as specified at `scale = 1`.
pub fn presets(scale: u32) -> Vec<Preset> {
    let u = 1_000_000u32.saturating_mul(scale);
    let len = 100_000u32.saturating_mul(scale);
    let ulists = 64;
    vec![
        Preset { name: "uniform-d0.0001", source: Source::Uniform { universe: u, lists: 64, density: 0.0001 } },
        Preset { name: "uniform-d0.001", source: Source::Uniform { universe: u, lists: 64, density: 0.001 } },
        Preset { name: "uniform-d0.01", source: Source::Uniform { universe: u, lists: 64, density: 0.01 } },
        Preset { name: "uniform-d0.1", source: Source::Uniform { universe: u, lists: 32, density: 0.1 } },
        Preset { name: "uniform-d0.5", source: Source::Uniform { universe: u, lists: 8, density: 0.5 } },
        Preset {
            name: "clustered-d0.01-b8",
            source: Source::Clustered { universe: u, lists: 64, density: 0.01, burst_len: 8.0 },
        },
        Preset {
            name: "clustered-d0.01-b64",
            source: Source::Clustered { universe: u, lists: 64, density: 0.01, burst_len: 64.0 },
        },
        Preset {
            name: "clustered-d0.01-b512",
            source: Source::Clustered { universe: u, lists: 64, density: 0.01, burst_len: 512.0 },
        },
        Preset {
            name: "clustered-d0.1-b64",
            source: Source::Clustered { universe: u, lists: 32, density: 0.1, burst_len: 64.0 },
        },
        Preset { name: "periodic-s100-j0", source: Source::Periodic { universe: u, lists: 64, stride: 100, jitter: 0 } },
        Preset {
            name: "periodic-s100-j10",
            source: Source::Periodic { universe: u, lists: 64, stride: 100, jitter: 10 },
        },
        Preset { name: "runs-d0.1-r32", source: Source::Runs { universe: u, lists: 32, density: 0.1, run_len: 32.0 } },
        Preset { name: "runs-d0.5-r256", source: Source::Runs { universe: u, lists: 8, density: 0.5, run_len: 256.0 } },
        Preset {
            name: "zipf-dict-e1-b0",
            source: Source::ZipfDictionary { universe: u, lists: 20_000, exponent: 1.0, max_density: 0.2, burst_len: 0.0 },
        },
        Preset {
            name: "zipf-dict-e1-b64",
            source: Source::ZipfDictionary {
                universe: u,
                lists: 20_000,
                exponent: 1.0,
                max_density: 0.2,
                burst_len: 64.0,
            },
        },
        Preset { name: "geometric-p0.5", source: Source::Geometric { lists: ulists, len, p: 0.5 } },
        Preset { name: "geometric-p0.1", source: Source::Geometric { lists: ulists, len, p: 0.1 } },
        Preset { name: "uniform-bits-4", source: Source::UniformBits { lists: ulists, len, bits: 4 } },
        Preset { name: "uniform-bits-12", source: Source::UniformBits { lists: ulists, len, bits: 12 } },
        Preset { name: "uniform-bits-20", source: Source::UniformBits { lists: ulists, len, bits: 20 } },
        Preset { name: "zipf-values-n1024-e1.2", source: Source::ZipfValues { lists: ulists, len, n: 1024, exponent: 1.2 } },
    ]
}

// ── generate ──

pub fn generate(name: &str, source: &Source, seed: u64) -> Dataset {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut ds = Dataset::new(name, source.kind(), source.universe(), source.describe());
    match source {
        Source::Uniform { universe, lists, density } => {
            for _ in 0..*lists {
                ds.lists.push(uniform_list(&mut rng, *universe, *density));
            }
        }
        Source::Clustered { universe, lists, density, burst_len } => {
            for _ in 0..*lists {
                ds.lists.push(clustered_list(&mut rng, *universe, *density, *burst_len));
            }
        }
        Source::Periodic { universe, lists, stride, jitter } => {
            for _ in 0..*lists {
                ds.lists.push(periodic_list(&mut rng, *universe, *stride, *jitter));
            }
        }
        Source::Runs { universe, lists, density, run_len } => {
            for _ in 0..*lists {
                ds.lists.push(runs_list(&mut rng, *universe, *density, *run_len));
            }
        }
        Source::ZipfDictionary { universe, lists, exponent, max_density, burst_len } => {
            let max_n = (*max_density * f64::from(*universe)).round().max(1.0);
            for rank in 1..=*lists {
                let n = zipf_rank_len(rank, *exponent, max_n).min(*universe as usize);
                let density = n as f64 / f64::from((*universe).max(1));
                let mut list = if *burst_len <= 0.0 {
                    uniform_list(&mut rng, *universe, density)
                } else {
                    clustered_list(&mut rng, *universe, density, *burst_len)
                };
                force_len(&mut rng, *universe, &mut list, n);
                ds.lists.push(list);
            }
        }
        Source::Geometric { lists, len, p } => {
            for _ in 0..*lists {
                ds.lists.push(geometric_list(&mut rng, *len, *p));
            }
        }
        Source::UniformBits { lists, len, bits } => {
            let universe = bits_universe(*bits);
            for _ in 0..*lists {
                ds.lists.push(uniform_bits_list(&mut rng, *len, universe));
            }
        }
        Source::ZipfValues { lists, len, n, exponent } => {
            for _ in 0..*lists {
                ds.lists.push(zipf_values_list(&mut rng, *len, *n, *exponent));
            }
        }
    }
    ds
}

// ── samplers ──

fn bits_universe(bits: u32) -> u32 {
    if bits == 0 {
        1
    } else if bits >= 32 {
        u32::MAX
    } else {
        1u32 << bits
    }
}

fn zipf_rank_len(rank: u32, exponent: f64, max_n: f64) -> usize {
    let len = (max_n / f64::from(rank.max(1)).powf(exponent)).round();
    len.max(1.0) as usize
}

/// Bernoulli sample of `[0, universe)`, generated by geometric gaps so cost
/// is proportional to output not universe.
fn uniform_list(rng: &mut StdRng, universe: u32, density: f64) -> Vec<u32> {
    let mut out = Vec::with_capacity((f64::from(universe) * density.clamp(0.0, 1.0)) as usize + 16);
    bernoulli_range(rng, 0, universe, density, &mut out);
    out
}

fn clustered_list(rng: &mut StdRng, universe: u32, density: f64, burst_len: f64) -> Vec<u32> {
    let d = density.clamp(0.0, 1.0);
    let l = burst_len.max(1.0);
    if universe == 0 || d <= 0.0 {
        return Vec::new();
    }
    if d >= 1.0 {
        return (0..universe).collect();
    }
    let alpha = (1.0 / l).min(1.0);
    let beta = (d / (l * (1.0 - d))).min(1.0);
    let p_on = (1.0 - (1.0 - d) / l).clamp(0.0, 1.0);
    let p_off = (d / l).clamp(0.0, 1.0);
    let mut out = Vec::with_capacity((f64::from(universe) * d) as usize + 16);
    let mut pos = 0u32;
    let mut on = rng.random_bool(d);
    while pos < universe {
        let leave = if on { alpha } else { beta };
        let sojourn = 1u64.saturating_add(geometric_failures(rng, leave));
        let end = u64::from(pos).saturating_add(sojourn).min(u64::from(universe)) as u32;
        bernoulli_range(rng, pos, end, if on { p_on } else { p_off }, &mut out);
        pos = end;
        on = !on;
    }
    out
}

fn periodic_list(rng: &mut StdRng, universe: u32, stride: u32, jitter: u32) -> Vec<u32> {
    let stride = stride.max(1);
    let mut out = Vec::new();
    let mut last: Option<u32> = None;
    let u = u64::from(universe);
    let j = u64::from(jitter);
    let mut k = 0u64;
    loop {
        let center = k.saturating_mul(u64::from(stride));
        if center > u.saturating_add(j) {
            break;
        }
        k += 1;
        let lo = -(i64::from(jitter));
        let hi = i64::from(jitter);
        let noise = if lo <= hi { rng.random_range(lo..=hi) } else { 0 };
        let Some(v) = (center as i64).checked_add(noise) else {
            continue;
        };
        if v < 0 || v >= i64::from(universe) {
            continue;
        }
        let v = v as u32;
        if let Some(prev) = last
            && v <= prev
        {
            continue;
        }
        out.push(v);
        last = Some(v);
    }
    out
}

fn runs_list(rng: &mut StdRng, universe: u32, density: f64, run_len: f64) -> Vec<u32> {
    let d = density.clamp(0.0, 1.0);
    let l = run_len.max(1.0);
    if universe == 0 || d <= 0.0 {
        return Vec::new();
    }
    if d >= 1.0 {
        return (0..universe).collect();
    }
    let p_leave_run = (1.0 / l).min(1.0);
    let e_gap = l * (1.0 - d) / d;
    let p_leave_gap = (1.0 / e_gap).min(1.0);
    let mut out = Vec::with_capacity((f64::from(universe) * d) as usize + 16);
    let mut pos = 0u64;
    let mut in_run = rng.random_bool(d);
    let u = u64::from(universe);
    while pos < u {
        if in_run {
            let r = 1u64.saturating_add(geometric_failures(rng, p_leave_run));
            let end = pos.saturating_add(r).min(u);
            for v in pos..end {
                out.push(v as u32);
            }
            pos = end;
            in_run = false;
        } else {
            let g = 1u64.saturating_add(geometric_failures(rng, p_leave_gap));
            pos = pos.saturating_add(g);
            in_run = true;
        }
    }
    out
}

fn geometric_list(rng: &mut StdRng, len: u32, p: f64) -> Vec<u32> {
    let p = p.clamp(0.0, 1.0);
    let Ok(g) = Geometric::new(p) else {
        return vec![1; len as usize];
    };
    (0..len as usize)
        .map(|_| {
            let x = 1u64.saturating_add(g.sample(rng));
            x.min(u64::from(u32::MAX - 1)) as u32
        })
        .collect()
}

fn uniform_bits_list(rng: &mut StdRng, len: u32, universe: u32) -> Vec<u32> {
    if universe == 0 {
        return Vec::new();
    }
    (0..len as usize).map(|_| rng.random_range(0..universe)).collect()
}

fn zipf_values_list(rng: &mut StdRng, len: u32, n: u32, exponent: f64) -> Vec<u32> {
    let n = n.max(1);
    let Ok(z) = Zipf::new(f64::from(n), exponent.max(0.0)) else {
        return vec![0; len as usize];
    };
    (0..len as usize)
        .map(|_| {
            let x: f64 = z.sample(rng);
            let rank = if x.is_finite() { x.max(1.0) as u32 } else { 1 };
            rank.saturating_sub(1).min(n - 1)
        })
        .collect()
}

fn bernoulli_range(rng: &mut StdRng, start: u32, end: u32, p: f64, out: &mut Vec<u32>) {
    if p <= 0.0 || start >= end {
        return;
    }
    if p >= 1.0 {
        out.extend(start..end);
        return;
    }
    let mut pos = u64::from(start).saturating_add(geometric_failures(rng, p));
    let end = u64::from(end);
    while pos < end {
        out.push(pos as u32);
        pos = pos.saturating_add(1).saturating_add(geometric_failures(rng, p));
    }
}

/// Number of failures before the first success at probability `p`.
fn geometric_failures(rng: &mut StdRng, p: f64) -> u64 {
    if p >= 1.0 {
        return 0;
    }
    if p <= 0.0 {
        return u64::MAX / 4;
    }
    match Geometric::new(p) {
        Ok(g) => g.sample(rng),
        Err(_) => 0,
    }
}

fn force_len(rng: &mut StdRng, universe: u32, list: &mut Vec<u32>, n: usize) {
    let n = n.min(universe as usize);
    if list.len() > n {
        list.truncate(n);
        return;
    }
    if list.len() == n || universe == 0 {
        return;
    }
    let mut seen: HashSet<u32> = list.iter().copied().collect();
    let mut attempts = 0usize;
    while list.len() < n && attempts < n.saturating_mul(32).max(32) {
        attempts += 1;
        let v = rng.random_range(0..universe);
        if seen.insert(v) {
            list.push(v);
        }
    }
    if list.len() < n {
        for v in 0..universe {
            if list.len() >= n {
                break;
            }
            if seen.insert(v) {
                list.push(v);
            }
        }
    }
    list.sort_unstable();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn density_of(list: &[u32], universe: u32) -> f64 {
        list.len() as f64 / f64::from(universe.max(1))
    }

    fn mean_run_len(list: &[u32]) -> f64 {
        if list.is_empty() {
            return 0.0;
        }
        let mut runs = 1usize;
        for w in list.windows(2) {
            if w[1] != w[0] + 1 {
                runs += 1;
            }
        }
        list.len() as f64 / runs as f64
    }

    #[test]
    fn uniform_density_is_close() {
        let ds = generate("t", &Source::Uniform { universe: 100_000, lists: 4, density: 0.01 }, 1);
        assert!(ds.validate().is_ok());
        for l in &ds.lists {
            let d = density_of(l, 100_000);
            assert!((0.008..0.012).contains(&d), "density {d}");
        }
    }

    #[test]
    fn clustered_density_is_close() {
        let ds = generate(
            "t",
            &Source::Clustered { universe: 100_000, lists: 4, density: 0.05, burst_len: 32.0 },
            1,
        );
        assert!(ds.validate().is_ok());
        for l in &ds.lists {
            let d = density_of(l, 100_000);
            assert!((0.04..0.06).contains(&d), "density {d}");
        }
    }

    #[test]
    fn periodic_is_strictly_increasing_and_on_grid() {
        let s = Source::Periodic { universe: 10_000, lists: 2, stride: 10, jitter: 0 };
        let ds = generate("t", &s, 3);
        assert!(ds.validate().is_ok());
        for l in &ds.lists {
            assert!(l.windows(2).all(|w| w[1] == w[0] + 10), "gaps {:?}", l.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>());
            assert_eq!(l.first().copied(), Some(0));
        }
        let s = Source::Periodic { universe: 10_000, lists: 2, stride: 10, jitter: 3 };
        let ds = generate("t", &s, 3);
        assert!(ds.validate().is_ok());
        for l in &ds.lists {
            assert!(l.windows(2).all(|w| w[0] < w[1]));
            assert!(density_of(l, 10_000) > 0.05);
        }
    }

    #[test]
    fn runs_have_expected_mean_length() {
        let ds = generate("t", &Source::Runs { universe: 100_000, lists: 4, density: 0.1, run_len: 32.0 }, 2);
        assert!(ds.validate().is_ok());
        for l in &ds.lists {
            let d = density_of(l, 100_000);
            assert!((0.08..0.12).contains(&d), "density {d}");
            let m = mean_run_len(l);
            assert!((24.0..40.0).contains(&m), "mean run {m}");
        }
    }

    #[test]
    fn zipf_dictionary_lengths_are_monotone() {
        let universe = 10_000;
        let lists = 20;
        let exponent = 1.0;
        let max_density = 0.2;
        let ds = generate(
            "t",
            &Source::ZipfDictionary { universe, lists, exponent, max_density, burst_len: 0.0 },
            4,
        );
        assert!(ds.validate().is_ok());
        let max_n = (max_density * f64::from(universe)).round().max(1.0);
        for (i, l) in ds.lists.iter().enumerate() {
            let want = zipf_rank_len((i as u32) + 1, exponent, max_n).min(universe as usize);
            assert_eq!(l.len(), want, "rank {}", i + 1);
            assert!(!l.is_empty());
        }
        assert!(ds.lists.windows(2).all(|w| w[0].len() >= w[1].len()));
    }

    #[test]
    fn geometric_values_are_at_least_one() {
        let ds = generate("t", &Source::Geometric { lists: 2, len: 10_000, p: 0.5 }, 5);
        assert!(ds.validate().is_ok());
        for l in &ds.lists {
            assert!(l.iter().all(|&v| v >= 1));
            assert_eq!(l.len(), 10_000);
        }
    }

    #[test]
    fn uniform_bits_values_in_range() {
        let ds = generate("t", &Source::UniformBits { lists: 2, len: 10_000, bits: 12 }, 6);
        assert!(ds.validate().is_ok());
        assert_eq!(ds.meta.universe, 1 << 12);
        for l in &ds.lists {
            assert!(l.iter().all(|&v| v < 1 << 12));
            assert_eq!(l.len(), 10_000);
        }
    }

    #[test]
    fn zipf_values_in_range() {
        let ds = generate("t", &Source::ZipfValues { lists: 2, len: 10_000, n: 1024, exponent: 1.2 }, 7);
        assert!(ds.validate().is_ok());
        assert_eq!(ds.meta.universe, 1024);
        for l in &ds.lists {
            assert!(l.iter().all(|&v| v < 1024));
            assert_eq!(l.len(), 10_000);
        }
    }

    #[test]
    fn every_preset_validates() {
        let mut total = 0usize;
        for p in presets(1) {
            let ds = generate(p.name, &p.source, 42);
            assert!(ds.validate().is_ok(), "{}: {:?}", p.name, ds.validate().err());
            assert!(!ds.lists.is_empty(), "{}", p.name);
            total += ds.total_ints();
        }
        assert!(total < 100_000_000, "scale-1 presets produced {total} ints");
    }

    #[test]
    fn deterministic_for_seed() {
        let sources = [
            Source::Uniform { universe: 10_000, lists: 2, density: 0.05 },
            Source::Clustered { universe: 10_000, lists: 2, density: 0.05, burst_len: 16.0 },
            Source::Periodic { universe: 10_000, lists: 2, stride: 7, jitter: 2 },
            Source::Runs { universe: 10_000, lists: 2, density: 0.2, run_len: 8.0 },
            Source::ZipfDictionary { universe: 10_000, lists: 8, exponent: 1.0, max_density: 0.2, burst_len: 0.0 },
            Source::Geometric { lists: 2, len: 100, p: 0.3 },
            Source::UniformBits { lists: 2, len: 100, bits: 8 },
            Source::ZipfValues { lists: 2, len: 100, n: 50, exponent: 1.1 },
        ];
        for s in sources {
            assert_eq!(generate("a", &s, 7).lists, generate("a", &s, 7).lists, "{}", s.describe());
        }
    }
}
