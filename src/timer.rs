//! Repeated timing with a median, plus hardware cycle counts where the OS
//! lets us have them.
//!
//! Wall-clock on a laptop-class CPU is noisy under boost and governor
//! changes; cycles are not, so both are reported. Cycle counting uses
//! `perf_event_open` restricted to user space, which works with the default
//! `perf_event_paranoid = 2` — no root needed.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// A timed quantity: wall-clock nanoseconds and (if available) CPU cycles,
/// both the median over `runs` repetitions, with the spread as a noise
/// indicator.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Sample {
    pub runs: u32,
    pub ns: f64,
    /// Interquartile range over median of the wall-clock runs; > 0.1 means
    /// the machine was noisy and the run is worth repeating.
    pub ns_spread: f64,
    pub cycles: Option<f64>,
}

/// Repeat `f` until at least `min_runs` and either `budget` elapsed or
/// `max_runs`; take the median. `f` must do the whole unit of work each call.
pub fn measure(min_runs: u32, max_runs: u32, budget: Duration, mut f: impl FnMut()) -> Sample {
    let mut counter = Cycles::open();
    let mut ns = Vec::new();
    let mut cycles = Vec::new();
    let start = Instant::now();
    // Warm-up: one untimed run for page faults, allocator growth, branch state.
    f();
    while (ns.len() as u32) < min_runs || (start.elapsed() < budget && (ns.len() as u32) < max_runs) {
        let c0 = counter.as_mut().map(Cycles::read);
        let t0 = Instant::now();
        f();
        let dt = t0.elapsed();
        if let (Some(c), Some(c0)) = (counter.as_mut(), c0) {
            cycles.push((c.read() - c0) as f64);
        }
        ns.push(dt.as_secs_f64() * 1e9);
    }
    ns.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);
    let med = ns[ns.len() / 2];
    let (q1, q3) = (ns[ns.len() / 4], ns[ns.len() * 3 / 4]);
    let spread = if med > 0.0 { (q3 - q1) / med } else { 0.0 };
    Sample {
        runs: ns.len() as u32,
        ns: med,
        ns_spread: spread,
        cycles: if cycles.is_empty() { None } else { Some(cycles[cycles.len() / 2]) },
    }
}

#[cfg(feature = "perf")]
struct Cycles(perf_event::Counter);

#[cfg(feature = "perf")]
impl Cycles {
    fn open() -> Option<Self> {
        let mut c = perf_event::Builder::new().kind(perf_event::events::Hardware::CPU_CYCLES).build().ok()?;
        c.enable().ok()?;
        Some(Self(c))
    }
    fn read(&mut self) -> u64 {
        self.0.read().unwrap_or(0)
    }
}

#[cfg(not(feature = "perf"))]
struct Cycles;

#[cfg(not(feature = "perf"))]
impl Cycles {
    fn open() -> Option<Self> {
        None
    }
    fn read(&mut self) -> u64 {
        0
    }
}

/// Whether hardware cycle counting is available on this machine, for the
/// report header.
pub fn cycles_available() -> bool {
    Cycles::open().is_some()
}

// ── peak memory ──

/// Reset the process's peak-RSS counter (Linux `clear_refs`). Returns
/// `false` if unsupported, in which case [`peak_rss_kb`] deltas are
/// meaningless and should be reported as unknown.
pub fn reset_peak_rss() -> bool {
    std::fs::write("/proc/self/clear_refs", "5\n").is_ok()
}

/// Current peak RSS in KiB (`VmHWM`), since the last [`reset_peak_rss`].
pub fn peak_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|l| l.strip_prefix("VmHWM:")).and_then(|v| v.trim().trim_end_matches(" kB").parse().ok())
}

/// Current RSS in KiB (`VmRSS`).
pub fn rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|l| l.strip_prefix("VmRSS:")).and_then(|v| v.trim().trim_end_matches(" kB").parse().ok())
}
