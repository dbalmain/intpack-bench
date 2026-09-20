//! Corpus extraction: turn a directory of files into the stream types a
//! content index would actually store, so the benchmark can be re-run on
//! real data by pointing it at a real tree.
//!
//! Walk is recursive, no symlink following, files only. Paths whose directory
//! components are VCS/build noise are skipped, as are files larger than 8 MiB
//! and files whose first 8 KiB contain a NUL. Remaining files are sorted by
//! full path; that order *is* the docID assignment, so postings see directory
//! locality the way a path-ordered index would.
//!
//! Datasets, in the order [`run`] returns them:
//!
//! 1. `words.docs` — [`Kind::Sorted`], universe = n_docs. For each distinct
//!    word (maximal `[A-Za-z0-9_]` run, ASCII-lowercased, length 2..=64), the
//!    sorted docIDs containing it.
//! 2. `trigrams.docs` — [`Kind::Sorted`], universe = n_docs. For each distinct
//!    overlapping byte trigram (raw bytes, Cox/Google Code Search), the sorted
//!    docIDs containing it.
//! 3. `words.freqs` — [`Kind::Unsorted`], universe = `u32::MAX`. Term frequency
//!    in each containing doc, in docID order. 1:1 with `words.docs`.
//! 4. `words.posdeltas` — [`Kind::Unsorted`], universe = `u32::MAX`. Per word,
//!    concatenation over its docs of in-doc positions as Lucene `.pos` deltas:
//!    first position in each doc stored as-is, subsequent as `pos - prev` (≥ 1).
//!    Position is the index among accepted words in the file, from 0.
//! 5. `docs.sizes` — [`Kind::Unsorted`], one list of file sizes in bytes.
//! 6. `docs.mtimes` — [`Kind::Unsorted`], one list of mtime seconds since epoch
//!    minus the corpus minimum.
//!
//! Sampling keeps a term iff `fnv1a(seed || term) % D == 0`, with `D` chosen
//! from a first-pass posting count so the kept total is ≈ `max_ints`. The same
//! kept-set is applied to the three word datasets. `docs.*` are never sampled.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result, bail};

use crate::stream::{Dataset, Kind};

/// Directories that dominate a developer tree by bytes and swamp the signal.
const SKIP: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    ".venv",
    "venv",
    "__pycache__",
    ".cache",
    "dist",
    "build",
    ".next",
    "vendor",
    ".direnv",
    "result",
    ".cargo",
    ".rustup",
    ".npm",
    ".m2",
    ".gradle",
];

const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const PROBE_BYTES: usize = 8 * 1024;
const WORD_MIN: usize = 2;
const WORD_MAX: usize = 64;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;

#[derive(Clone, Debug)]
pub struct Options {
    /// Deterministic term sampling keeps roughly this many ints per dataset
    /// while preserving the list-length distribution.
    pub max_ints: usize,
    pub seed: u64,
}

/// Walk `root` and produce the datasets described in the module doc.
pub fn run(root: &Path, opts: &Options) -> Result<Vec<Dataset>> {
    if !root.exists() {
        bail!("{} does not exist", root.display());
    }

    let files = collect_files(root)?;
    let Ok(n_docs) = u32::try_from(files.len()) else {
        bail!("more than u32::MAX documents");
    };

    let (word_postings, trigram_postings) = count_postings(&files)?;
    let d_words = sample_d(word_postings, opts.max_ints);
    let d_trigrams = sample_d(trigram_postings, opts.max_ints);

    let source = format!(
        "{}; exclude={}; max_file=8MiB; binary=NUL-in-first-8KiB; sample=fnv1a(term,seed)%D==0 seed={} max_ints={} D_words={} D_trigrams={}",
        root.display(),
        SKIP.join(","),
        opts.seed,
        opts.max_ints,
        d_words,
        d_trigrams,
    );

    let mut words: HashMap<Vec<u8>, TermAcc> = HashMap::new();
    let mut trigrams: HashMap<[u8; 3], Vec<u32>> = HashMap::new();
    let mut per_doc: HashMap<Vec<u8>, Vec<u32>> = HashMap::new();
    let mut tri_seen: HashSet<[u8; 3]> = HashSet::new();

    for (i, file) in files.iter().enumerate() {
        let doc = i as u32;
        let bytes = fs::read(&file.path).with_context(|| format!("read {}", file.path.display()))?;

        per_doc.clear();
        for_each_word(&bytes, |w, pos| {
            if !keep_term(w, opts.seed, d_words) {
                return;
            }
            match per_doc.get_mut(w) {
                Some(positions) => positions.push(pos),
                None => {
                    per_doc.insert(w.to_vec(), vec![pos]);
                }
            }
        });
        for (term, positions) in per_doc.drain() {
            let freq = u32::try_from(positions.len()).unwrap_or(u32::MAX - 1);
            let acc = words.entry(term).or_default();
            acc.docs.push(doc);
            acc.freqs.push(freq);
            append_deltas(&mut acc.posdeltas, &positions);
        }

        tri_seen.clear();
        for t in trigram_iter(&bytes) {
            if !keep_term(&t, opts.seed, d_trigrams) || !tri_seen.insert(t) {
                continue;
            }
            trigrams.entry(t).or_default().push(doc);
        }
    }

    let mut word_terms: Vec<(Vec<u8>, TermAcc)> = words.into_iter().collect();
    word_terms.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let mut words_docs = Dataset::new("words.docs", Kind::Sorted, n_docs, source.clone());
    let mut words_freqs = Dataset::new("words.freqs", Kind::Unsorted, u32::MAX, source.clone());
    let mut words_pos = Dataset::new("words.posdeltas", Kind::Unsorted, u32::MAX, source.clone());
    words_docs.lists.reserve(word_terms.len());
    words_freqs.lists.reserve(word_terms.len());
    words_pos.lists.reserve(word_terms.len());
    for (_, acc) in word_terms {
        words_docs.lists.push(acc.docs);
        words_freqs.lists.push(acc.freqs);
        words_pos.lists.push(acc.posdeltas);
    }

    let mut tri_terms: Vec<([u8; 3], Vec<u32>)> = trigrams.into_iter().collect();
    tri_terms.sort_unstable_by_key(|a| a.0);
    let mut tri_docs = Dataset::new("trigrams.docs", Kind::Sorted, n_docs, source.clone());
    tri_docs.lists.reserve(tri_terms.len());
    for (_, list) in tri_terms {
        tri_docs.lists.push(list);
    }

    let mut sizes = Dataset::new("docs.sizes", Kind::Unsorted, u32::MAX, source.clone());
    sizes.lists.push(files.iter().map(|f| f.size).collect());

    let min_mtime = files.iter().map(|f| f.mtime).min().unwrap_or(0);
    let mut mtimes = Dataset::new("docs.mtimes", Kind::Unsorted, u32::MAX, source);
    mtimes
        .lists
        .push(files.iter().map(|f| u32::try_from(f.mtime.saturating_sub(min_mtime)).unwrap_or(u32::MAX - 1)).collect());

    Ok(vec![words_docs, tri_docs, words_freqs, words_pos, sizes, mtimes])
}

// ── walk ──

struct FileInfo {
    path: PathBuf,
    size: u32,
    mtime: u64,
}

fn skip_name(name: &OsStr) -> bool {
    name.to_str().is_some_and(|s| SKIP.contains(&s))
}

fn collect_files(root: &Path) -> Result<Vec<FileInfo>> {
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !skip_name(e.file_name()));

    let mut files = Vec::new();
    for ent in walker {
        let Ok(ent) = ent else { continue };
        if !ent.file_type().is_file() {
            continue;
        }
        let Ok(meta) = ent.metadata() else { continue };
        if meta.len() > MAX_FILE_BYTES {
            continue;
        }
        match looks_binary(ent.path()) {
            Ok(true) | Err(_) => continue,
            Ok(false) => {}
        }
        let size = u32::try_from(meta.len()).unwrap_or(u32::MAX - 1);
        let mtime = meta.modified().ok().map(unix_secs).unwrap_or(0);
        files.push(FileInfo { path: ent.into_path(), size, mtime });
    }
    // Path order is the locality a directory-tree index would see; shuffling
    // would turn bursty postings into uniform noise.
    files.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn looks_binary(path: &Path) -> Result<bool> {
    let mut f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut buf = [0u8; PROBE_BYTES];
    let n = f.read(&mut buf).with_context(|| format!("read {}", path.display()))?;
    Ok(buf[..n].contains(&0))
}

fn unix_secs(t: std::time::SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ── tokenise ──

fn is_word_byte(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_')
}

/// Call `f(lowercased_word, position)` for each indexed word.
/// Position counts accepted words only (length 2..=64); short/long runs are dropped.
fn for_each_word(bytes: &[u8], mut f: impl FnMut(&[u8], u32)) {
    let mut pos = 0u32;
    let mut i = 0;
    while i < bytes.len() {
        if !is_word_byte(bytes[i]) {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() && is_word_byte(bytes[i]) {
            i += 1;
        }
        let n = i - start;
        if !(WORD_MIN..=WORD_MAX).contains(&n) {
            continue;
        }
        let mut buf = [0u8; WORD_MAX];
        for (j, &b) in bytes[start..i].iter().enumerate() {
            buf[j] = b.to_ascii_lowercase();
        }
        f(&buf[..n], pos);
        pos = pos.saturating_add(1);
    }
}

fn trigram_iter(bytes: &[u8]) -> impl Iterator<Item = [u8; 3]> + '_ {
    bytes.windows(3).map(|w| [w[0], w[1], w[2]])
}

// ── sampling ──

/// FNV-1a 64-bit over `seed` (little-endian) followed by `term`.
fn fnv1a(seed: u64, term: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    for b in seed.to_le_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    for &b in term {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// Keep-modulus so expected kept postings are at most `max_ints`.
fn sample_d(total_postings: usize, max_ints: usize) -> u64 {
    if max_ints == 0 {
        return 0;
    }
    // Ceiling, so `max_ints` is a cap rather than a value the kept count
    // can overshoot by up to 2x.
    total_postings.div_ceil(max_ints).max(1) as u64
}

fn keep_term(term: &[u8], seed: u64, d: u64) -> bool {
    d != 0 && fnv1a(seed, term).is_multiple_of(d)
}

// ── accumulate ──

#[derive(Default)]
struct TermAcc {
    docs: Vec<u32>,
    freqs: Vec<u32>,
    posdeltas: Vec<u32>,
}

/// First position in each doc is stored raw; a delta from the previous doc
/// would mix two universes (position vs gap) in one stream.
fn append_deltas(out: &mut Vec<u32>, positions: &[u32]) {
    let Some((&first, rest)) = positions.split_first() else {
        return;
    };
    out.push(first);
    let mut prev = first;
    for &p in rest {
        out.push(p.saturating_sub(prev));
        prev = p;
    }
}

fn count_postings(files: &[FileInfo]) -> Result<(usize, usize)> {
    let mut word_postings = 0usize;
    let mut trigram_postings = 0usize;
    let mut uniq_words: HashSet<Vec<u8>> = HashSet::new();
    let mut uniq_tri: HashSet<[u8; 3]> = HashSet::new();
    for file in files {
        let bytes = fs::read(&file.path).with_context(|| format!("read {}", file.path.display()))?;
        uniq_words.clear();
        for_each_word(&bytes, |w, _| {
            uniq_words.insert(w.to_vec());
        });
        word_postings += uniq_words.len();
        uniq_tri.clear();
        uniq_tri.extend(trigram_iter(&bytes));
        trigram_postings += uniq_tri.len();
    }
    Ok((word_postings, trigram_postings))
}

// ── tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    struct TmpDir(PathBuf);

    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn tmp(name: &str) -> Result<TmpDir> {
        let p = std::env::temp_dir().join(format!("ipb-extract-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p)?;
        Ok(TmpDir(p))
    }

    fn dataset<'a>(out: &'a [Dataset], name: &str) -> &'a Dataset {
        match out.iter().find(|d| d.meta.name == name) {
            Some(d) => d,
            None => panic!("missing dataset {name}"),
        }
    }

    fn set_mtime(path: &Path, secs: u64) -> Result<()> {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
        fs::OpenOptions::new().write(true).open(path)?.set_modified(t)?;
        Ok(())
    }

    fn find_unique_list(ds: &Dataset, want: &[u32]) -> usize {
        let hits: Vec<usize> =
            ds.lists.iter().enumerate().filter(|(_, l)| l.as_slice() == want).map(|(i, _)| i).collect();
        assert_eq!(hits.len(), 1, "expected exactly one list {want:?}, found {}", hits.len());
        hits[0]
    }

    /// Sorted path order of the six kept files, used as docIDs.
    ///
    /// 0 a/one.txt      "alpha beta alpha"
    /// 1 a/two.txt      "beta gamma"
    /// 2 b/five.txt     "epsilon zeta"
    /// 3 b/four.txt     "fouruniq fouruniq"
    /// 4 b/three.txt    "alpha"
    /// 5 six.txt        "alpha beta"
    const ONE: &[u8] = b"alpha beta alpha";
    const TWO: &[u8] = b"beta gamma";
    const FIVE: &[u8] = b"epsilon zeta";
    const FOUR: &[u8] = b"fouruniq fouruniq";
    const THREE: &[u8] = b"alpha";
    const SIX: &[u8] = b"alpha beta";
    const T0: u64 = 1_700_000_000;

    fn write_fixture(root: &Path) -> Result<()> {
        fs::create_dir_all(root.join("a"))?;
        fs::create_dir_all(root.join("b"))?;
        fs::create_dir_all(root.join("node_modules"))?;
        fs::write(root.join("a/one.txt"), ONE)?;
        fs::write(root.join("a/two.txt"), TWO)?;
        fs::write(root.join("b/five.txt"), FIVE)?;
        fs::write(root.join("b/four.txt"), FOUR)?;
        fs::write(root.join("b/three.txt"), THREE)?;
        fs::write(root.join("six.txt"), SIX)?;
        fs::write(root.join("binary.bin"), b"hello\0world")?;
        fs::write(root.join("node_modules/pkg.js"), b"nmonlyterm")?;
        let rels = ["a/one.txt", "a/two.txt", "b/five.txt", "b/four.txt", "b/three.txt", "six.txt"];
        for (i, rel) in rels.iter().enumerate() {
            set_mtime(&root.join(rel), T0 + 10 * i as u64)?;
        }
        Ok(())
    }

    #[test]
    fn tokenise_drops_short_runs_without_occupying_a_position() {
        let mut got = Vec::new();
        for_each_word(b"Alpha _x a beta2", |w, p| got.push((w.to_vec(), p)));
        assert_eq!(got, vec![(b"alpha".to_vec(), 0), (b"_x".to_vec(), 1), (b"beta2".to_vec(), 2)]);
    }

    #[test]
    fn posdeltas_restart_per_doc() {
        let mut out = Vec::new();
        append_deltas(&mut out, &[0, 3, 10]);
        append_deltas(&mut out, &[2, 4]);
        assert_eq!(out, vec![0, 3, 7, 2, 2]);
    }

    #[test]
    fn fnv1a_is_seeded_and_deterministic() {
        assert_eq!(fnv1a(42, b"alpha"), fnv1a(42, b"alpha"));
        assert_ne!(fnv1a(42, b"alpha"), fnv1a(43, b"alpha"));
        assert_ne!(fnv1a(42, b"alpha"), fnv1a(42, b"beta"));
    }

    #[test]
    fn sample_d_keeps_everything_when_under_budget() {
        assert_eq!(sample_d(10, 100), 1);
        assert_eq!(sample_d(100, 100), 1);
        assert_eq!(sample_d(101, 100), 2);
        assert_eq!(sample_d(250, 100), 3);
        assert_eq!(sample_d(1, 0), 0);
    }

    #[test]
    fn fixture_streams_match_hand_computation() -> Result<()> {
        let dir = tmp("corpus")?;
        write_fixture(&dir.0)?;
        let out = run(&dir.0, &Options { max_ints: 50_000_000, seed: 1 })?;
        assert_eq!(out.len(), 6);

        let words = dataset(&out, "words.docs");
        let freqs = dataset(&out, "words.freqs");
        let pos = dataset(&out, "words.posdeltas");
        let tri = dataset(&out, "trigrams.docs");
        let sizes = dataset(&out, "docs.sizes");
        let mtimes = dataset(&out, "docs.mtimes");

        words.validate()?;
        freqs.validate()?;
        pos.validate()?;
        tri.validate()?;
        sizes.validate()?;
        mtimes.validate()?;

        assert_eq!(words.meta.kind, Kind::Sorted);
        assert_eq!(words.meta.universe, 6);
        assert_eq!(tri.meta.universe, 6);
        assert_eq!(freqs.meta.kind, Kind::Unsorted);
        assert_eq!(pos.meta.kind, Kind::Unsorted);
        assert_eq!(sizes.meta.kind, Kind::Unsorted);
        assert_eq!(mtimes.meta.kind, Kind::Unsorted);

        // Binary and node_modules/ are absent: six docs, six terms.
        // alpha beta gamma epsilon zeta fouruniq — nmonlyterm / hello / world not present.
        assert_eq!(words.lists.len(), 6);
        assert_eq!(freqs.lists.len(), 6);
        assert_eq!(pos.lists.len(), 6);

        // fouruniq lives only in b/four.txt, which is doc 3 under sorted paths.
        let i = find_unique_list(words, &[3]);
        assert_eq!(freqs.lists[i], vec![2]);
        assert_eq!(pos.lists[i], vec![0, 1]);

        // alpha in a/one.txt (doc 0, pos 0 and 2), b/three.txt (doc 4, pos 0),
        // six.txt (doc 5, pos 0).
        let i = find_unique_list(words, &[0, 4, 5]);
        assert_eq!(freqs.lists[i], vec![2, 1, 1]);
        assert_eq!(pos.lists[i], vec![0, 2, 0, 0]);

        // beta in a/one.txt (doc 0, pos 1), a/two.txt (doc 1, pos 0), six.txt (doc 5, pos 1).
        let i = find_unique_list(words, &[0, 1, 5]);
        assert_eq!(freqs.lists[i], vec![1, 1, 1]);
        assert_eq!(pos.lists[i], vec![1, 0, 1]);

        // Trigram "alp" (from "alpha") is in docs 0, 4, 5.
        assert!(tri.lists.iter().any(|l| l.as_slice() == [0, 4, 5]), "missing trigram list for 'alp'");
        // Distinctive trigram "fou" only in four.txt.
        assert!(tri.lists.iter().any(|l| l.as_slice() == [3]), "missing trigram list for four.txt");

        assert_eq!(sizes.lists.len(), 1);
        assert_eq!(mtimes.lists.len(), 1);
        assert_eq!(
            sizes.lists[0],
            vec![
                ONE.len() as u32,
                TWO.len() as u32,
                FIVE.len() as u32,
                FOUR.len() as u32,
                THREE.len() as u32,
                SIX.len() as u32
            ]
        );
        assert_eq!(mtimes.lists[0], vec![0, 10, 20, 30, 40, 50]);
        Ok(())
    }

    #[test]
    fn sampling_is_deterministic_aligned_and_seed_sensitive() -> Result<()> {
        let dir = tmp("sample")?;
        for i in 0..80u32 {
            fs::write(dir.0.join(format!("t{i:02}.txt")), format!("shared w{i:02}"))?;
        }
        let opts = Options { max_ints: 40, seed: 7 };
        let a = run(&dir.0, &opts)?;
        let b = run(&dir.0, &opts)?;
        let c = run(&dir.0, &Options { max_ints: 40, seed: 8 })?;

        let a_docs = dataset(&a, "words.docs");
        let b_docs = dataset(&b, "words.docs");
        let c_docs = dataset(&c, "words.docs");
        assert_eq!(a_docs.lists, b_docs.lists, "same seed must keep the same terms");
        assert_ne!(a_docs.lists, c_docs.lists, "different seed must keep a different set");

        let a_tri = dataset(&a, "trigrams.docs");
        let b_tri = dataset(&b, "trigrams.docs");
        let c_tri = dataset(&c, "trigrams.docs");
        assert_eq!(a_tri.lists, b_tri.lists);
        assert_ne!(a_tri.lists, c_tri.lists);

        let freqs = dataset(&a, "words.freqs");
        let pos = dataset(&a, "words.posdeltas");
        assert_eq!(a_docs.lists.len(), freqs.lists.len());
        assert_eq!(a_docs.lists.len(), pos.lists.len());
        for i in 0..a_docs.lists.len() {
            assert_eq!(a_docs.lists[i].len(), freqs.lists[i].len(), "docs/freqs misaligned at {i}");
            let tf: u64 = freqs.lists[i].iter().map(|&x| u64::from(x)).sum();
            assert_eq!(tf, pos.lists[i].len() as u64, "freqs/posdeltas misaligned at {i}");
        }

        let kept: usize = a_docs.lists.iter().map(Vec::len).sum();
        assert!(kept > 0 && kept < 160, "kept {kept} word postings, expected a subsample of 160");

        let sizes = dataset(&a, "docs.sizes");
        assert_eq!(sizes.lists.len(), 1);
        assert_eq!(sizes.lists[0].len(), 80, "docs.* must not be sampled");
        Ok(())
    }
}
