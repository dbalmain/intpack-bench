//! Dataset container and on-disk format.
//!
//! A dataset is a named bag of integer lists sharing one universe. Sorted
//! datasets model posting lists (docIDs, positions); unsorted ones model
//! small-integer payloads (term frequencies, position deltas, doc values).
//!
//! File layout (`.ipb`, little-endian):
//!
//! ```text
//! magic  b"IPB1"
//! u32    metadata JSON length, then that many bytes of JSON ([`Meta`])
//! u32    list count
//! per list: u32 len, then len × u32 values
//! ```
//!
//! The format is deliberately trivial so a harness in any language can read it.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const MAGIC: &[u8; 4] = b"IPB1";

/// What a dataset models; decides which operations the harness runs on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Strictly increasing values in `[0, universe)`. Skip/seek and
    /// intersection are meaningful.
    Sorted,
    /// Arbitrary non-negative values; only bulk encode/decode and random
    /// access apply.
    Unsorted,
}

/// Provenance and shape of a dataset, stored in the file header.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    pub name: String,
    pub kind: Kind,
    /// Exclusive upper bound on values. For sorted lists this is the document
    /// count; the entropy bound depends on it.
    pub universe: u32,
    /// Free-form description of how the data was produced (generator
    /// parameters or corpus path + extraction rules), for the report.
    pub source: String,
}

#[derive(Clone, Debug)]
pub struct Dataset {
    pub meta: Meta,
    pub lists: Vec<Vec<u32>>,
}

impl Dataset {
    pub fn new(name: impl Into<String>, kind: Kind, universe: u32, source: impl Into<String>) -> Self {
        Self { meta: Meta { name: name.into(), kind, universe, source: source.into() }, lists: Vec::new() }
    }

    pub fn total_ints(&self) -> usize {
        self.lists.iter().map(Vec::len).sum()
    }

    /// Validate the invariants the codecs rely on: sorted lists are strictly
    /// increasing, and every value is below the universe.
    pub fn validate(&self) -> Result<()> {
        for (i, list) in self.lists.iter().enumerate() {
            if let Some(&max) = list.iter().max()
                && max >= self.meta.universe
            {
                bail!("list {i}: value {max} >= universe {}", self.meta.universe);
            }
            if self.meta.kind == Kind::Sorted && !list.windows(2).all(|w| w[0] < w[1]) {
                bail!("list {i}: not strictly increasing");
            }
        }
        Ok(())
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        let file = File::create(path).with_context(|| format!("create {}", path.display()))?;
        let mut w = BufWriter::new(file);
        let meta = serde_json::to_vec(&self.meta)?;
        w.write_all(MAGIC)?;
        w.write_all(&(meta.len() as u32).to_le_bytes())?;
        w.write_all(&meta)?;
        w.write_all(&(self.lists.len() as u32).to_le_bytes())?;
        for list in &self.lists {
            w.write_all(&(list.len() as u32).to_le_bytes())?;
            for &v in list {
                w.write_all(&v.to_le_bytes())?;
            }
        }
        w.flush()?;
        Ok(())
    }

    pub fn read(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
        let mut r = BufReader::new(file);
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if &magic != MAGIC {
            bail!("{}: not an IPB1 file", path.display());
        }
        let meta_len = read_u32(&mut r)? as usize;
        let mut meta = vec![0u8; meta_len];
        r.read_exact(&mut meta)?;
        let meta: Meta = serde_json::from_slice(&meta)?;
        let n_lists = read_u32(&mut r)? as usize;
        let mut lists = Vec::with_capacity(n_lists);
        for _ in 0..n_lists {
            let len = read_u32(&mut r)? as usize;
            let mut bytes = vec![0u8; len * 4];
            r.read_exact(&mut bytes)?;
            lists.push(bytes.as_chunks::<4>().0.iter().map(|&c| u32::from_le_bytes(c)).collect());
        }
        Ok(Self { meta, lists })
    }
}

fn read_u32(r: &mut impl Read) -> Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

// ── entropy bounds ──

/// Information-theoretic lower bound, in bits, for a list under a model that
/// assumes nothing about clustering.
///
/// Sorted: `log2(C(universe, n))` — the bits needed to name one `n`-subset
/// of the universe. Any codec beating this on average is exploiting structure
/// (runs, clustering) the uniform model does not have; the report shows the
/// codec's bits/int *minus* this bound so that "how much structure did it
/// find" and "how much overhead did it pay" are visible separately.
///
/// Unsorted: zeroth-order empirical entropy of the values, `n · H(p)`, the
/// bound for any codec that treats values as i.i.d. draws.
pub fn entropy_bits(kind: Kind, universe: u32, list: &[u32]) -> f64 {
    match kind {
        Kind::Sorted => log2_binomial(universe as f64, list.len() as f64),
        Kind::Unsorted => empirical_entropy_bits(list),
    }
}

/// `log2(C(n, k))` via log-gamma, accurate enough for reporting.
fn log2_binomial(n: f64, k: f64) -> f64 {
    if k <= 0.0 || k >= n {
        return 0.0;
    }
    (ln_gamma(n + 1.0) - ln_gamma(k + 1.0) - ln_gamma(n - k + 1.0)) / std::f64::consts::LN_2
}

/// Lanczos approximation of `ln Γ(x)` for `x > 0`.
fn ln_gamma(x: f64) -> f64 {
    const G: f64 = 7.0;
    const COEF: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        let pi = std::f64::consts::PI;
        return (pi / (pi * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = COEF[0];
    let t = x + G + 0.5;
    for (i, c) in COEF.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

fn empirical_entropy_bits(list: &[u32]) -> f64 {
    if list.is_empty() {
        return 0.0;
    }
    let mut sorted = list.to_vec();
    sorted.sort_unstable();
    let n = list.len() as f64;
    let mut bits = 0.0;
    let mut i = 0;
    while i < sorted.len() {
        let mut j = i;
        while j < sorted.len() && sorted[j] == sorted[i] {
            j += 1;
        }
        let p = (j - i) as f64 / n;
        bits -= (j - i) as f64 * p.log2();
        i = j;
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binomial_bound_matches_small_cases() {
        // C(10, 3) = 120
        assert!((log2_binomial(10.0, 3.0) - 120f64.log2()).abs() < 1e-6);
        // C(1000, 1) = 1000
        assert!((log2_binomial(1000.0, 1.0) - 1000f64.log2()).abs() < 1e-6);
    }

    #[test]
    fn entropy_of_constant_list_is_zero() {
        assert_eq!(empirical_entropy_bits(&[7, 7, 7, 7]), 0.0);
    }

    #[test]
    fn entropy_of_uniform_bits_is_one_per_value() {
        assert!((empirical_entropy_bits(&[0, 1, 0, 1]) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn roundtrip_file() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("ipb-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("t.ipb");
        let mut ds = Dataset::new("t", Kind::Sorted, 100, "test");
        ds.lists.push(vec![1, 5, 9]);
        ds.lists.push(vec![]);
        ds.lists.push(vec![99]);
        ds.write(&path)?;
        let back = Dataset::read(&path)?;
        assert_eq!(back.lists, ds.lists);
        assert_eq!(back.meta.universe, 100);
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn validate_rejects_unsorted_sorted_kind() {
        let mut ds = Dataset::new("t", Kind::Sorted, 100, "test");
        ds.lists.push(vec![5, 1]);
        assert!(ds.validate().is_err());
    }
}
