//! Scratch driver for one intersection ratio. Not a supported bench mode.
//!
//! Pair selection replays `bench::run`'s rng (conformance sample, then ratios
//! 1, 10, 100, 1000 in order) and keeps the requested ratio, so the 64 pairs
//! are the ones the harness times. The long-cursor trace follows the current
//! skip search: the current block, then a binary search from block 0, or a
//! gallop of 1, 2, 4, … blocks ahead of any later block.
//!
//! Build this only behind the `ix-profile` feature. A release bench that
//! links the module changes thin-LTO codegen of the cursors: the same
//! words.docs ∩ 1:1000 went from 115 / 88 / 56 ns to 89 / 106 / 56. Block
//! stats (`--only stats`) do not use those timings. The feature does not
//! turn on rdtsc probes inside `next_geq`; those probes are larger than a
//! 1:10 call, and the phase split is perf plus the full/long/short wall
//! clock below.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hint::black_box;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use rand::prelude::*;

use crate::codec::bp128skip::Bp128Skip;
use crate::codec::lucene::for_util;
use crate::codec::lucene::io::Reader;
use crate::codec::{Codec, Cursor};
use crate::stream::{Dataset, Kind};
use crate::timer;

const BLOCK: usize = 128;
/// pfor's bitset token; the walk reports such a block as width `BITSET`,
/// no exceptions.
const BITSET: u8 = 0xfe;

pub fn run(
    path: &std::path::Path,
    ratio: u32,
    budget_s: f64,
    seed: u64,
    perf_secs: f64,
    codec: &str,
    only: &str,
) -> Result<()> {
    if !matches!(ratio, 1 | 10 | 100 | 1000) {
        bail!("--ratio must be 1, 10, 100, or 1000");
    }
    if !matches!(only, "all" | "stats" | "wall") {
        bail!("--only must be all, stats, or wall");
    }
    eprintln!("== {}", machine_line());
    eprintln!("loading {}", path.display());
    let t0 = Instant::now();
    let ds = Dataset::read(path)?;
    eprintln!(
        "{}: {} lists, {} ints, universe {}, loaded in {:.2}s",
        ds.meta.name,
        ds.lists.len(),
        ds.total_ints(),
        ds.meta.universe,
        t0.elapsed().as_secs_f64()
    );

    let pairs = select_pairs(&ds, seed, ratio)?;
    let short_ints: usize = pairs.iter().map(|&(s, _)| ds.lists[s].len()).sum();
    let long_ints: usize = pairs.iter().map(|&(_, l)| ds.lists[l].len()).sum();
    check_fingerprint(&ds.meta.name, ratio, short_ints, long_ints)?;
    let mut shorts: Vec<usize> = pairs.iter().map(|&(s, _)| ds.lists[s].len()).collect();
    let mut longs: Vec<usize> = pairs.iter().map(|&(_, l)| ds.lists[l].len()).collect();
    shorts.sort_unstable();
    longs.sort_unstable();
    println!(
        "ratio={ratio} pairs={} short_ints={} long_ints={} actual={:.2} short_len[{} {} {}] long_len[{} {} {}]",
        pairs.len(),
        short_ints,
        long_ints,
        long_ints as f64 / short_ints as f64,
        shorts[0],
        shorts[shorts.len() / 2],
        shorts[shorts.len() - 1],
        longs[0],
        longs[longs.len() / 2],
        longs[longs.len() - 1]
    );

    if perf_secs > 0.0 {
        let lists = unique_lists(&pairs);
        let enc = encode(codec, &ds, &lists)?;
        return perf_loop(&enc, &pairs, perf_secs);
    }

    eprintln!("simulating leapfrog");
    let mut traces = Vec::with_capacity(pairs.len());
    for (pi, &(s, l)) in pairs.iter().enumerate() {
        let (calls, ops) = drive(&ds.lists[s], &ds.lists[l]);
        let naive = naive_count(&ds.lists[s], &ds.lists[l]);
        let hits = calls.iter().filter(|c| c.y == Some(c.target)).count();
        if hits != naive {
            bail!("pair {pi}: trace hits {hits} != naive {naive}");
        }
        traces.push((calls, ops));
    }

    let lists = unique_lists(&pairs);
    let long_lists = unique_side(&pairs, false);
    eprintln!("encoding {} lists ({} long) with three codecs", lists.len(), long_lists.len());
    let t_enc = Instant::now();
    let pfor = encode("ip-pfor128-skip", &ds, &lists)?;
    let bp = encode("bp128-skip", &ds, &lists)?;
    let lucene = encode("lucene-docs", &ds, &lists)?;
    eprintln!("encoded in {:.2}s", t_enc.elapsed().as_secs_f64());

    eprintln!("checking traces against cursors");
    check_cursors(&ds, &pfor, &pairs, &traces)?;

    let blocks = walk_blocks(&ds, &pfor, &lucene, &long_lists)?;
    print_block_stats(&ds, &pairs, &traces, &blocks, &pfor, &bp, &lucene);

    if only == "stats" {
        return Ok(());
    }

    let budget = Duration::from_secs_f64(budget_s);
    let micro = Duration::from_secs_f64((budget_s * 0.5).max(0.35));
    println!("\n# wall clock (ns per short-list element, median, budget {budget_s}s)");
    for (name, enc) in [("ip-pfor128-skip", &pfor), ("bp128-skip", &bp), ("lucene-docs", &lucene)] {
        let full = time_it(budget, short_ints, || replay_full(enc, &pairs));
        let long = time_it(budget, short_ints, || replay_long(enc, &pairs, &traces));
        let short = time_it(budget, short_ints, || replay_short(enc, &pairs, &traces));
        let per_long = |ns: f64| ns * short_ints as f64 / long_ints as f64;
        println!(
            "{name:16} full {}  long {}  short {}  frame {:+.2}  per-long-e full {:.2} long {:.2} short {:.2}",
            fmt_timed(full),
            fmt_timed(long),
            fmt_timed(short),
            full.ns - long.ns - short.ns,
            per_long(full.ns),
            per_long(long.ns),
            per_long(short.ns)
        );
    }

    print_micro(&ds, &pfor, &bp, &lists, &pairs, &traces, &blocks, short_ints, micro)?;
    Ok(())
}

fn machine_line() -> String {
    let host = std::fs::read_to_string("/etc/hostname").unwrap_or_default();
    let cpu = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let model = cpu.lines().find_map(|l| l.strip_prefix("model name\t: ")).unwrap_or("unknown");
    let gov = std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor").unwrap_or_default();
    format!("{} {} governor {}", host.trim(), model.trim(), gov.trim())
}

fn select_pairs(ds: &Dataset, seed: u64, want: u32) -> Result<Vec<(usize, usize)>> {
    let cfg = crate::bench::Config::default();
    let mut rng = StdRng::seed_from_u64(seed);
    if ds.lists.len() > cfg.check_lists {
        for _ in 0..cfg.check_lists / 2 {
            let _ = rng.random_range(0..ds.lists.len());
        }
    }
    let mut by_len: Vec<usize> = (0..ds.lists.len()).filter(|&i| ds.lists[i].len() >= 32).collect();
    by_len.sort_by_key(|&i| ds.lists[i].len());
    let lens: Vec<usize> = by_len.iter().map(|&i| ds.lists[i].len()).collect();
    let mut out = None;
    // Draw every ratio up to the one requested. Later draws do not move
    // this ratio's pairs, and stopping early keeps the rng aligned with
    // `bench::run`, which draws 1, then 10, then 100, then 1000.
    for ratio in [1u32, 10, 100, 1000] {
        let pairs = ratio_pairs(&by_len, &lens, ratio, &mut rng);
        if ratio == want {
            out = pairs;
            break;
        }
    }
    out.with_context(|| format!("ratio {want} produced no pairs"))
}

fn ratio_pairs(by_len: &[usize], lens: &[usize], ratio: u32, rng: &mut StdRng) -> Option<Vec<(usize, usize)>> {
    let mut pairs = Vec::new();
    let mut attempts = 0;
    while pairs.len() < 64 && attempts < 4096 {
        attempts += 1;
        let s = rng.random_range(0..by_len.len());
        let want = lens[s] * ratio as usize;
        let p = lens.partition_point(|&l| l < want);
        let cands = [p.saturating_sub(1), p, p + 1];
        let l = *cands.iter().filter(|&&c| c < lens.len() && c != s).min_by_key(|&&c| lens[c].abs_diff(want))?;
        let actual = lens[l] as f64 / lens[s] as f64;
        if actual < ratio as f64 * 0.5 || actual > ratio as f64 * 2.0 {
            continue;
        }
        pairs.push((by_len[s], by_len[l]));
    }
    if pairs.is_empty() { None } else { Some(pairs) }
}

fn check_fingerprint(name: &str, ratio: u32, short_ints: usize, long_ints: usize) -> Result<()> {
    // Only ratio 1000 has a published pair fingerprint. Other ratios replay
    // the same rng, so they match the harness, but there is no fixed size
    // to assert.
    if ratio != 1000 {
        return Ok(());
    }
    let expect = match name {
        "words.docs" => Some((8308, 7_090_620)),
        "trigrams.docs" => Some((8835, 7_708_159)),
        _ => None,
    };
    if let Some((s, l)) = expect
        && (s, l) != (short_ints, long_ints)
    {
        bail!("{name}: pair fingerprint {short_ints}/{long_ints}, expected {s}/{l}");
    }
    Ok(())
}

fn unique_lists(pairs: &[(usize, usize)]) -> Vec<usize> {
    let mut v: Vec<usize> = pairs.iter().flat_map(|&(s, l)| [s, l]).collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn unique_side(pairs: &[(usize, usize)], short: bool) -> Vec<usize> {
    let mut v: Vec<usize> = pairs.iter().map(|&(s, l)| if short { s } else { l }).collect();
    v.sort_unstable();
    v.dedup();
    v
}

struct Call {
    target: u32,
    /// `None` when the long cursor exhausted on this target.
    y: Option<u32>,
    early: bool,
    tail: bool,
    fresh: bool,
    from: usize,
    block: usize,
    start: usize,
    hit: usize,
    /// Block-last comparisons in the skip search. Zero on an early-out.
    steps: u64,
    /// Elements the long cursor consumed on this call.
    advanced: u32,
}

#[derive(Clone, Copy)]
enum ShortOp {
    Next,
    Geq(u32),
}

struct LState {
    pos: usize,
    cur: Option<u32>,
    in_tail: bool,
    loaded: bool,
    block: usize,
}

fn drive(short: &[u32], long: &[u32]) -> (Vec<Call>, Vec<ShortOp>) {
    let mut calls = Vec::new();
    let mut ops = Vec::new();
    if short.is_empty() || long.is_empty() {
        return (calls, ops);
    }
    let mut st = LState { pos: 0, cur: None, in_tail: false, loaded: false, block: 0 };
    ops.push(ShortOp::Next);
    let mut i = 1usize;
    let mut x = short[0];
    while let Some(y) = long_geq(long, &mut st, x, &mut calls) {
        if y == x {
            if i >= short.len() {
                ops.push(ShortOp::Next);
                break;
            }
            ops.push(ShortOp::Next);
            x = short[i];
            i += 1;
        } else {
            ops.push(ShortOp::Geq(y));
            let cur_i = i - 1;
            match short[cur_i..].iter().position(|&v| v >= y) {
                Some(off) => {
                    let ni = cur_i + off;
                    x = short[ni];
                    i = ni + 1;
                }
                None => break,
            }
        }
    }
    (calls, ops)
}

fn block_last(list: &[u32], b: usize) -> u32 {
    list[b * BLOCK + BLOCK - 1]
}

/// [`intpack::pfor128skip`]'s `Table::find`, counted in block-last comparisons.
fn find_block(list: &[u32], from: usize, target: u32) -> (usize, u64) {
    let blocks = list.len() / BLOCK;
    if from >= blocks {
        return (blocks, 0);
    }
    let mut steps = 1u64;
    if block_last(list, from) >= target {
        return (from, steps);
    }
    if from == 0 {
        return binsearch_counted(list, 1, blocks, target, steps);
    }
    let mut prev = from;
    let mut step = 1usize;
    loop {
        let probe = from.saturating_add(step);
        steps += 1;
        if probe >= blocks || block_last(list, probe) >= target {
            let hi = if probe >= blocks { blocks } else { probe + 1 };
            return binsearch_counted(list, prev + 1, hi, target, steps);
        }
        prev = probe;
        step = step.saturating_mul(2);
    }
}

fn binsearch_counted(list: &[u32], mut lo: usize, mut hi: usize, target: u32, mut steps: u64) -> (usize, u64) {
    while lo < hi {
        steps += 1;
        let mid = lo + (hi - lo) / 2;
        if block_last(list, mid) < target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    (lo, steps)
}

fn long_geq(list: &[u32], st: &mut LState, target: u32, calls: &mut Vec<Call>) -> Option<u32> {
    if let Some(c) = st.cur
        && c >= target
    {
        calls.push(Call {
            target,
            y: Some(c),
            early: true,
            tail: false,
            fresh: false,
            from: 0,
            block: 0,
            start: 0,
            hit: 0,
            steps: 0,
            advanced: 0,
        });
        return Some(c);
    }
    if st.in_tail {
        return scan_tail(list, st, target, false, 0, calls);
    }
    let blocks = list.len() / BLOCK;
    let from = st.pos.saturating_sub(1) / BLOCK;
    let (lo, steps) = find_block(list, from, target);
    if lo == blocks {
        return scan_tail(list, st, target, true, steps, calls);
    }
    let start = if lo == from { st.pos % BLOCK } else { 0 };
    let fresh = !st.loaded || st.block != lo;
    st.loaded = true;
    st.block = lo;
    let prev_pos = st.pos;
    for j in start..BLOCK {
        if list[lo * BLOCK + j] >= target {
            let y = list[lo * BLOCK + j];
            st.pos = lo * BLOCK + j + 1;
            st.cur = Some(y);
            calls.push(Call {
                target,
                y: Some(y),
                early: false,
                tail: false,
                fresh,
                from,
                block: lo,
                start,
                hit: j,
                steps,
                advanced: (st.pos - prev_pos) as u32,
            });
            return Some(y);
        }
    }
    calls.push(Call {
        target,
        y: None,
        early: false,
        tail: false,
        fresh: false,
        from,
        block: lo,
        start,
        hit: 0,
        steps,
        advanced: 0,
    });
    None
}

fn scan_tail(
    list: &[u32],
    st: &mut LState,
    target: u32,
    enter: bool,
    steps: u64,
    calls: &mut Vec<Call>,
) -> Option<u32> {
    let blocks = list.len() / BLOCK;
    let from = st.pos.saturating_sub(1) / BLOCK;
    let prev_pos = st.pos;
    if enter {
        st.pos = blocks * BLOCK;
        st.in_tail = true;
    }
    while st.pos < list.len() {
        let y = list[st.pos];
        st.pos += 1;
        st.cur = Some(y);
        if y >= target {
            calls.push(Call {
                target,
                y: Some(y),
                early: false,
                tail: true,
                fresh: false,
                from,
                block: blocks,
                start: 0,
                hit: 0,
                steps,
                advanced: (st.pos - prev_pos) as u32,
            });
            return Some(y);
        }
    }
    st.cur = None;
    calls.push(Call {
        target,
        y: None,
        early: false,
        tail: true,
        fresh: false,
        from,
        block: blocks,
        start: 0,
        hit: 0,
        steps,
        advanced: (st.pos - prev_pos) as u32,
    });
    None
}

fn naive_count(a: &[u32], b: &[u32]) -> usize {
    let (mut i, mut j, mut n) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                n += 1;
                i += 1;
                j += 1;
            }
        }
    }
    n
}

struct Encoded {
    bytes: Vec<u8>,
    /// Per dataset-list: `(off, len, n)`.
    span: Vec<Option<(usize, usize, usize)>>,
    codec: Box<dyn Codec>,
    universe: u32,
}

impl Encoded {
    fn cursor(&self, list: usize) -> Option<Box<dyn Cursor + '_>> {
        let (off, len, n) = self.span[list]?;
        self.codec.cursor(self.universe, n, &self.bytes[off..off + len])
    }

    fn bytes(&self, list: usize) -> Option<&[u8]> {
        let (off, len, _) = self.span[list]?;
        Some(&self.bytes[off..off + len])
    }
}

fn encode(name: &str, ds: &Dataset, lists: &[usize]) -> Result<Encoded> {
    let codec = crate::codec::by_name(name).with_context(|| format!("unknown codec {name}"))?;
    let mut bytes = Vec::new();
    let mut span = vec![None; ds.lists.len()];
    for &i in lists {
        let off = bytes.len();
        codec.encode(Kind::Sorted, ds.meta.universe, &ds.lists[i], &mut bytes);
        span[i] = Some((off, bytes.len() - off, ds.lists[i].len()));
    }
    Ok(Encoded { bytes, span, codec, universe: ds.meta.universe })
}

fn check_cursors(
    ds: &Dataset,
    enc: &Encoded,
    pairs: &[(usize, usize)],
    traces: &[(Vec<Call>, Vec<ShortOp>)],
) -> Result<()> {
    for (pi, (&(s, l), (calls, ops))) in pairs.iter().zip(traces).enumerate() {
        let Some(mut b) = enc.cursor(l) else { bail!("pair {pi}: no long cursor") };
        for (ci, c) in calls.iter().enumerate() {
            let got = b.next_geq(c.target);
            if got != c.y {
                bail!("pair {pi} long call {ci}: next_geq({}) = {got:?}, trace {:?}", c.target, c.y);
            }
        }
        let Some(mut a) = enc.cursor(s) else { bail!("pair {pi}: no short cursor") };
        let mut xs = Vec::new();
        for op in ops {
            let x = match *op {
                ShortOp::Next => a.next(),
                ShortOp::Geq(y) => a.next_geq(y),
            };
            if let Some(x) = x {
                xs.push(x);
            }
        }
        let targets: Vec<u32> = calls.iter().map(|c| c.target).collect();
        if xs != targets {
            bail!(
                "pair {pi}: short replay produced {} values, trace has {} targets (list {} len {})",
                xs.len(),
                targets.len(),
                s,
                ds.lists[s].len()
            );
        }
    }
    Ok(())
}

struct Blk {
    width: u8,
    exc: u8,
    token: i8,
    pfor_bytes: u16,
    lucene_bytes: u16,
}

fn walk_blocks(ds: &Dataset, pfor: &Encoded, lucene: &Encoded, longs: &[usize]) -> Result<HashMap<usize, Vec<Blk>>> {
    let mut out = HashMap::new();
    for &i in longs {
        let list = &ds.lists[i];
        let pb = pfor.bytes(i).context("missing pfor list")?;
        let lb = lucene.bytes(i).context("missing lucene list")?;
        let p = walk_pfor(list.len(), pb)?;
        let l = walk_lucene(list.len(), lb)?;
        if p.len() != l.len() {
            bail!("list {i}: pfor blocks {} != lucene {}", p.len(), l.len());
        }
        let mut blks = Vec::with_capacity(p.len());
        for (bi, ((width, exc, pbytes), (token, lbytes))) in p.into_iter().zip(l).enumerate() {
            let (pw, pe) = predict_pfor(list, bi);
            if (pw, pe) != (width, exc) {
                bail!("list {i} block {bi}: pfor token {width}/{exc}, predict {pw}/{pe}");
            }
            let pt = predict_lucene(list, bi);
            if pt != token {
                bail!("list {i} block {bi}: lucene token {token}, predict {pt}");
            }
            blks.push(Blk { width, exc, token, pfor_bytes: pbytes, lucene_bytes: lbytes });
        }
        out.insert(i, blks);
    }
    Ok(out)
}

fn walk_pfor(n: usize, buf: &[u8]) -> Result<Vec<(u8, u8, u16)>> {
    let blocks = n / BLOCK;
    let mut byte = blocks * 8;
    let mut out = Vec::with_capacity(blocks);
    for _ in 0..blocks {
        if byte >= buf.len() {
            bail!("pfor walk ran off the end");
        }
        let start = byte;
        let token = buf[byte];
        if token == 0xff {
            byte += 1 + BLOCK * 4;
            out.push((32, 255, (byte - start) as u16));
        } else if token == BITSET {
            byte += 1;
            while buf[byte] >= 0x80 {
                byte += 1;
            }
            byte += 1;
            byte += 1 + usize::from(buf[byte]);
            out.push((BITSET, 0, (byte - start) as u16));
        } else {
            let exc = token >> 5;
            let width = token & 31;
            byte += 1 + usize::from(exc) * 2 + usize::from(width) * 16;
            out.push((width, exc, (byte - start) as u16));
        }
    }
    Ok(out)
}

fn walk_lucene(n: usize, buf: &[u8]) -> Result<Vec<(i8, u16)>> {
    let mut r = Reader::new(buf);
    let blocks = n / BLOCK;
    let mut out = Vec::with_capacity(blocks);
    for i in 0..blocks {
        if i.is_multiple_of(32) && n - i * BLOCK >= 32 * BLOCK {
            let _ = r.read_vint();
            let _ = r.read_vlong();
        }
        let _ = r.read_vlong();
        let _ = r.read_vint15();
        let _ = r.read_vlong15();
        let token_b = r.read_byte();
        let token = token_b as i8;
        let start = r.pos - 1;
        if token > 0 {
            r.skip(for_util::num_bytes(u32::from(token_b)));
        } else if token < 0 {
            r.skip(usize::from(token.unsigned_abs()) * 8);
        }
        out.push((token, (r.pos - start) as u16));
    }
    Ok(out)
}

fn predict_pfor(list: &[u32], block: usize) -> (u8, u8) {
    let start = block * BLOCK;
    let mut prev = if block == 0 { None } else { Some(list[start - 1]) };
    let mut gaps = [0u32; BLOCK];
    for (i, gap) in gaps.iter_mut().enumerate() {
        let v = list[start + i];
        *gap = match prev {
            None => v,
            Some(p) => v - p - 1,
        };
        prev = Some(v);
    }
    let max_bits = 32 - gaps.iter().fold(0u32, |a, &v| a | v).leading_zeros();
    let mut copy = gaps;
    let (_, eighth, _) = copy.select_nth_unstable(BLOCK - 8);
    let width = (32 - eighth.leading_zeros()).max(max_bits.saturating_sub(8));
    let mask = if width >= 32 { u32::MAX } else { (1u32 << width) - 1 };
    let exceptions = gaps.iter().filter(|&&g| g > mask).count();
    if width == 32 || (width >= 30 && exceptions == 7) {
        return (32, 255);
    }
    let span = list[start + BLOCK - 1] - list[start];
    if span < 255 * 8 {
        let first_bytes = (32 - gaps[0].leading_zeros()).max(1).div_ceil(7) as usize;
        let bitset = 1 + first_bytes + 1 + span as usize / 8 + 1;
        if bitset < 1 + 2 * exceptions + width as usize * 16 {
            return (BITSET, 0);
        }
    }
    (width as u8, exceptions as u8)
}

fn predict_lucene(list: &[u32], block: usize) -> i8 {
    let start = block * BLOCK;
    let level0_last = if block == 0 { -1i64 } else { i64::from(list[start - 1]) };
    let mut prev = level0_last;
    let mut or_d = 0u32;
    for &doc in &list[start..start + BLOCK] {
        or_d |= (i64::from(doc) - prev) as u32;
        prev = i64::from(doc);
    }
    let doc_range = i64::from(list[start + BLOCK - 1]) - level0_last;
    let bpv = 1u32.max(32 - or_d.leading_zeros());
    let num_bits_next = i64::from(32.min(bpv + 1)) * BLOCK as i64;
    if doc_range == BLOCK as i64 {
        0
    } else if num_bits_next <= doc_range {
        bpv as i8
    } else {
        let num_longs = (doc_range as usize).div_ceil(64);
        -(num_longs as i8)
    }
}

fn print_block_stats(
    ds: &Dataset,
    pairs: &[(usize, usize)],
    traces: &[(Vec<Call>, Vec<ShortOp>)],
    blocks: &HashMap<usize, Vec<Blk>>,
    pfor: &Encoded,
    bp: &Encoded,
    lucene: &Encoded,
) {
    let mut early = 0u64;
    let mut tail = 0u64;
    let mut land = 0u64;
    let mut fresh = 0u64;
    let mut same = 0u64;
    let mut steps_find = 0u64;
    let mut steps_scan = 0u64;
    let mut jump_sum = 0u64;
    let mut jump_hist = [0u64; 8];
    let mut width_land = [0u64; 34];
    let mut width_fresh = [0u64; 34];
    let mut exc_land = 0u64;
    let mut exc_fresh = 0u64;
    let mut exc_sum_fresh = 0u64;
    let mut run_l = 0u64;
    let mut bit_l = 0u64;
    let mut pack_l = 0u64;
    let mut run_f = 0u64;
    let mut bit_f = 0u64;
    let mut pack_f = 0u64;
    let mut cross = [[0u64; 4]; 3]; // lucene kind × (w0, w1, w2plus, exc)
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut seen_run = 0u64;
    let mut seen_bit = 0u64;
    let mut seen_pack = 0u64;
    let mut seen_exc = 0u64;
    let mut hits = 0u64;
    let mut advanced = 0u64;
    let mut same_run = 0u64;
    let mut same_val = 0u64;
    let mut fresh_run = 0u64;
    let mut fresh_val = 0u64;
    let mut span_sum_val = 0u64;
    let mut span_hist = [0u64; 6];
    let mut steps_stay = 0u64;
    let mut n_stay = 0u64;
    let mut steps_move = 0u64;
    let mut n_move = 0u64;
    let mut short_next = 0u64;
    let mut short_geq = 0u64;

    for (_, ops) in traces {
        for op in ops {
            match op {
                ShortOp::Next => short_next += 1,
                ShortOp::Geq(_) => short_geq += 1,
            }
        }
    }

    for (&(_, l), (calls, _)) in pairs.iter().zip(traces) {
        let info = &blocks[&l];
        for c in calls {
            if c.y.is_none() {
                continue;
            }
            advanced += u64::from(c.advanced);
            if c.y == Some(c.target) {
                hits += 1;
            }
            if c.early {
                early += 1;
                continue;
            }
            steps_find += c.steps;
            if c.block == c.from {
                steps_stay += c.steps;
                n_stay += 1;
            } else {
                steps_move += c.steps;
                n_move += 1;
            }
            if c.tail {
                tail += 1;
                continue;
            }
            land += 1;
            let b = &info[c.block];
            let w = usize::from(b.width).min(33);
            width_land[w] += 1;
            let kind = lucene_kind(b.token);
            if b.exc > 0 {
                exc_land += 1;
            }
            if c.fresh {
                fresh += 1;
                width_fresh[w] += 1;
                if b.exc > 0 && b.exc != 255 {
                    exc_fresh += 1;
                    exc_sum_fresh += u64::from(b.exc);
                } else if b.exc == 255 {
                    exc_fresh += 1;
                }
                match kind {
                    0 => run_f += 1,
                    1 => bit_f += 1,
                    _ => pack_f += 1,
                }
            } else {
                same += 1;
            }
            match kind {
                0 => run_l += 1,
                1 => bit_l += 1,
                _ => pack_l += 1,
            }
            let our_run = b.width == 0 && b.exc == 0;
            if our_run {
                if c.fresh { fresh_run += 1 } else { same_run += 1 }
            } else if c.fresh {
                fresh_val += 1;
            } else {
                same_val += 1;
            }
            if !our_run {
                let span = (c.hit - c.start + 1) as u64;
                span_sum_val += span;
                let bucket = match span {
                    1..=4 => 0,
                    5..=8 => 1,
                    9..=16 => 2,
                    17..=32 => 3,
                    33..=64 => 4,
                    _ => 5,
                };
                span_hist[bucket] += 1;
            }
            let col = if b.exc > 0 {
                3
            } else if b.width == 0 {
                0
            } else if b.width == 1 {
                1
            } else {
                2
            };
            cross[kind][col] += 1;
            let delta = c.block.saturating_sub(c.from);
            jump_sum += delta as u64;
            jump_hist[jump_bucket(delta)] += 1;
            steps_scan += (c.hit - c.start + 1) as u64;
            if seen.insert((l, c.block)) {
                match kind {
                    0 => seen_run += 1,
                    1 => seen_bit += 1,
                    _ => seen_pack += 1,
                }
                if b.exc > 0 {
                    seen_exc += 1;
                }
            }
        }
    }

    let searches = land + tail;
    println!("\n# cursor events over the 64 long lists");
    println!(
        "long_calls={} early={} ({:.1}%) block_lands={} fresh_unpacks={} same_block={} tail={}",
        early + land + tail,
        early,
        pct(early, early + land + tail),
        land,
        fresh,
        same,
        tail
    );
    let calls_n = early + land + tail;
    println!(
        "hits={} ({:.1}% of calls) advanced/call={:.2} short_next={} short_geq={}",
        hits,
        pct(hits, calls_n),
        advanced as f64 / calls_n.max(1) as f64,
        short_next,
        short_geq
    );
    println!(
        "find_steps/search={:.2} stay_steps={:.2} (n={}) move_steps={:.2} (n={}) scan_steps/land={:.1} block_jump/land={:.2} jump_hist[0,1,2-3,4-7,8-15,16-31,32-63,64+]={}",
        steps_find as f64 / searches.max(1) as f64,
        steps_stay as f64 / n_stay.max(1) as f64,
        n_stay,
        steps_move as f64 / n_move.max(1) as f64,
        n_move,
        steps_scan as f64 / land.max(1) as f64,
        jump_sum as f64 / land.max(1) as f64,
        nums(&jump_hist)
    );
    println!(
        "our blocks: same-run={} same-values={} fresh-run={} fresh-values={}",
        same_run, same_val, fresh_run, fresh_val
    );
    println!(
        "value-block span/land={:.1} hist[1-4,5-8,9-16,17-32,33-64,65+]={}",
        span_sum_val as f64 / (same_val + fresh_val).max(1) as f64,
        nums(&span_hist)
    );

    println!("\n# block kinds (landing = every non-early block next_geq, weighted by visits)");
    println!("{:22} {:>10} {:>8} {:>8} {:>8} {:>8}", "set", "blocks", "exc%", "run%", "bitset%", "packed%");
    print_kind_row("landings", land, exc_land, run_l, bit_l, pack_l);
    print_kind_row("fresh unpacks", fresh, exc_fresh, run_f, bit_f, pack_f);
    print_kind_row("unique landed", seen.len() as u64, seen_exc, seen_run, seen_bit, seen_pack);

    let mut all = 0u64;
    let mut all_exc = 0u64;
    let mut all_run = 0u64;
    let mut all_bit = 0u64;
    let mut all_pack = 0u64;
    let mut width_all = [0u64; 34];
    let mut body_pfor = 0u64;
    let mut body_luc = 0u64;
    let mut body_replaced = 0u64;
    let mut unary_pfor_body = 0u64;
    let mut unary_luc_body = 0u64;
    let mut unary_n = 0u64;
    for blks in blocks.values() {
        for b in blks {
            all += 1;
            width_all[usize::from(b.width).min(33)] += 1;
            let kind = lucene_kind(b.token);
            if b.exc > 0 {
                all_exc += 1;
            }
            match kind {
                0 => all_run += 1,
                1 => all_bit += 1,
                _ => all_pack += 1,
            }
            body_pfor += u64::from(b.pfor_bytes);
            body_luc += u64::from(b.lucene_bytes);
            if b.token <= 0 {
                body_replaced += u64::from(b.lucene_bytes);
                unary_pfor_body += u64::from(b.pfor_bytes);
                unary_luc_body += u64::from(b.lucene_bytes);
                unary_n += 1;
            } else {
                body_replaced += u64::from(b.pfor_bytes);
            }
        }
    }
    print_kind_row("all blocks in long lists", all, all_exc, all_run, all_bit, all_pack);
    let long_unique_ints: usize = blocks.keys().map(|&i| ds.lists[i].len()).sum();
    let bits = |enc: &Encoded| -> f64 {
        let bytes: usize = blocks.keys().map(|&i| enc.span[i].map(|(_, len, _)| len).unwrap_or(0)).sum();
        bytes as f64 * 8.0 / long_unique_ints as f64
    };
    println!(
        "long-list bits/int (unique lists): pfor {:.3}  bp128 {:.3}  lucene {:.3}  (n={long_unique_ints})",
        bits(pfor),
        bits(bp),
        bits(lucene)
    );
    let d_bits = (body_replaced as f64 - body_pfor as f64) * 8.0 / long_unique_ints as f64;
    println!(
        "body bytes on long-list blocks: pfor {body_pfor} lucene {body_luc} pfor-with-unary-bodies-swapped {body_replaced} ({d_bits:+.3} bits/int)"
    );
    println!(
        "unary blocks {unary_n}: pfor body {:.2} B  lucene body {:.2} B",
        unary_pfor_body as f64 / unary_n.max(1) as f64,
        unary_luc_body as f64 / unary_n.max(1) as f64
    );
    if fresh > 0 && exc_fresh > 0 {
        println!("mean exceptions on fresh exc blocks: {:.2}", exc_sum_fresh as f64 / exc_fresh as f64);
    }
    println!("width hist landings: {}", fmt_hist(&width_land));
    println!("width hist fresh:    {}", fmt_hist(&width_fresh));
    println!("width hist all:      {}", fmt_hist(&width_all));
    println!("landing cross-tab rows=run,bitset,packed cols=w0,w1,w>=2,exc");
    for (name, row) in [("run", 0), ("bitset", 1), ("packed", 2)] {
        println!("  {name:7} {}", row_fmt(&cross[row]));
    }
}

fn lucene_kind(token: i8) -> usize {
    if token == 0 {
        0
    } else if token < 0 {
        1
    } else {
        2
    }
}

fn jump_bucket(d: usize) -> usize {
    match d {
        0 => 0,
        1 => 1,
        2..=3 => 2,
        4..=7 => 3,
        8..=15 => 4,
        16..=31 => 5,
        32..=63 => 6,
        _ => 7,
    }
}

fn print_kind_row(name: &str, n: u64, exc: u64, run: u64, bit: u64, pack: u64) {
    println!(
        "{name:22} {n:>10} {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}%",
        pct(exc, n),
        pct(run, n),
        pct(bit, n),
        pct(pack, n)
    );
}

fn fmt_hist(hist: &[u64]) -> String {
    let total: u64 = hist.iter().sum();
    let mut s = String::new();
    for (w, &n) in hist.iter().enumerate() {
        if n > 0 {
            s.push_str(&format!(" {w}:{}({:.0}%)", n, pct(n, total)));
        }
    }
    s
}

fn row_fmt(row: &[u64]) -> String {
    nums(row)
}

fn nums(row: &[u64]) -> String {
    row.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" ")
}

fn pct(n: u64, d: u64) -> f64 {
    if d == 0 { 0.0 } else { 100.0 * n as f64 / d as f64 }
}

#[derive(Clone, Copy)]
struct Timed {
    ns: f64,
    spread: f64,
    runs: u32,
    cycles: Option<f64>,
}

fn time_it(budget: Duration, short_ints: usize, mut f: impl FnMut()) -> Timed {
    let s = timer::measure(7, 100, budget, &mut f);
    Timed {
        ns: s.ns / short_ints as f64,
        spread: s.ns_spread,
        runs: s.runs,
        cycles: s.cycles.map(|c| c / short_ints as f64),
    }
}

fn fmt_timed(t: Timed) -> String {
    match t.cycles {
        Some(c) => format!("{:.1}ns spr{:.3} cyc{:.0} n{}", t.ns, t.spread, c, t.runs),
        None => format!("{:.1}ns spr{:.3} n{}", t.ns, t.spread, t.runs),
    }
}

fn replay_full(enc: &Encoded, pairs: &[(usize, usize)]) {
    for &(s, l) in pairs {
        let (Some(mut a), Some(mut b)) = (enc.cursor(s), enc.cursor(l)) else { continue };
        black_box(leapfrog(&mut *a, &mut *b));
    }
}

fn leapfrog(a: &mut dyn Cursor, b: &mut dyn Cursor) -> usize {
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
}

fn replay_long(enc: &Encoded, pairs: &[(usize, usize)], traces: &[(Vec<Call>, Vec<ShortOp>)]) {
    for (&(_, l), (calls, _)) in pairs.iter().zip(traces) {
        let Some(mut b) = enc.cursor(l) else { continue };
        for c in calls {
            black_box(b.next_geq(c.target));
        }
    }
}

fn replay_short(enc: &Encoded, pairs: &[(usize, usize)], traces: &[(Vec<Call>, Vec<ShortOp>)]) {
    for (&(s, _), (_, ops)) in pairs.iter().zip(traces) {
        let Some(mut a) = enc.cursor(s) else { continue };
        for op in ops {
            match *op {
                ShortOp::Next => black_box(a.next()),
                ShortOp::Geq(y) => black_box(a.next_geq(y)),
            };
        }
    }
}

fn perf_loop(enc: &Encoded, pairs: &[(usize, usize)], secs: f64) -> Result<()> {
    let start = Instant::now();
    let mut passes = 0u64;
    while start.elapsed().as_secs_f64() < secs {
        replay_full(enc, pairs);
        passes += 1;
    }
    eprintln!("perf loop {passes} passes in {:.2}s", start.elapsed().as_secs_f64());
    Ok(())
}

struct Slot {
    n: usize,
    off: usize,
    len: usize,
}

struct DecItem {
    slot: usize,
    index: usize,
}

struct ScanItem {
    block: [u32; BLOCK],
    target: u32,
    start: usize,
    hit: usize,
}

struct BitItem {
    words: [u64; 64],
    num_longs: usize,
    base: i64,
    from: i64,
    expect: u32,
}

struct SearchItem {
    slot: usize,
    from: usize,
    blocks: usize,
    target: u32,
    expect: usize,
}

// The inputs are the measurement tables already built above; packing them
// further would only rename the same values.
#[allow(clippy::too_many_arguments)]
fn print_micro(
    ds: &Dataset,
    pfor: &Encoded,
    bp: &Encoded,
    lists: &[usize],
    pairs: &[(usize, usize)],
    traces: &[(Vec<Call>, Vec<ShortOp>)],
    blocks: &HashMap<usize, Vec<Blk>>,
    short_ints: usize,
    budget: Duration,
) -> Result<()> {
    let slot_of: HashMap<usize, usize> = lists.iter().enumerate().map(|(s, &i)| (i, s)).collect();
    let pfor_slots = slots_for(pfor, lists);
    let bp_slots = slots_for(bp, lists);
    let mut dec_clean = Vec::new();
    let mut dec_exc = Vec::new();
    let mut scans = Vec::new();
    let mut scans_unary = Vec::new();
    let mut bits = Vec::new();
    let mut searches = Vec::new();

    for (&(_, l), (calls, _)) in pairs.iter().zip(traces) {
        let info = &blocks[&l];
        let slot = slot_of[&l];
        let list = &ds.lists[l];
        let n_blocks = list.len() / BLOCK;
        // Table last-values must be the block maxima the cursor searches.
        let buf = pfor.bytes(l).context("pfor bytes")?;
        for b in 0..n_blocks {
            let got = table_last(buf, b);
            if got != list[b * BLOCK + BLOCK - 1] {
                bail!("list {l} block {b}: table last {got} != {}", list[b * BLOCK + BLOCK - 1]);
            }
        }
        for c in calls {
            if c.early || c.y.is_none() {
                continue;
            }
            searches.push(SearchItem {
                slot,
                from: c.from,
                blocks: n_blocks,
                target: c.target,
                expect: if c.tail { n_blocks } else { c.block },
            });
            if c.tail {
                continue;
            }
            let b = &info[c.block];
            let item = DecItem { slot, index: c.block * BLOCK };
            if c.fresh {
                if b.exc > 0 { dec_exc.push(item) } else { dec_clean.push(item) }
            }
            let mut block = [0u32; BLOCK];
            block.copy_from_slice(&list[c.block * BLOCK..c.block * BLOCK + BLOCK]);
            let scan = ScanItem { block, target: c.target, start: c.start, hit: c.hit };
            if b.token <= 0 {
                scans_unary.push(scan_clone(&scan));
                bits.push(bit_item(list, c, b.token)?);
            }
            scans.push(scan);
        }
    }

    for s in &scans {
        if scalar_hit(&s.block, s.target, s.start) != s.hit {
            bail!("scalar scan mismatch");
        }
        if simd_hit(&s.block, s.target, s.start) != s.hit {
            bail!("simd scan mismatch");
        }
    }
    for b in &bits {
        let got = next_set_bit(&b.words, b.num_longs, b.from);
        let doc = got.map(|bit| b.base + bit);
        if doc != Some(i64::from(b.expect)) {
            bail!("next_set_bit {doc:?} != {}", b.expect);
        }
    }
    let pfor_bytes = &pfor.bytes;
    for q in &searches {
        let buf = slot_buf(pfor_bytes, &pfor_slots[q.slot]);
        let got = binary_search(buf, q.from, q.blocks, q.target);
        if got != q.expect {
            bail!("binary search {got} != {}", q.expect);
        }
        let g = table_find(buf, q.from, q.blocks, q.target);
        if g != q.expect {
            bail!("table find {g} != {}", q.expect);
        }
    }
    let avx512 = std::arch::is_x86_feature_detected!("avx512f");
    if avx512 {
        for s in &scans {
            // SAFETY: the runtime check above is `avx512f`, which this kernel uses.
            let hit = unsafe { simd_hit_avx512(&s.block, s.target, s.start) };
            if hit != s.hit {
                bail!("avx512 scan {hit} != {}", s.hit);
            }
        }
    }

    println!("\n# microbench ns/e (one pass over the recorded events, median)");
    let t_clean_p = time_decode(budget, short_ints, &dec_clean, pfor_bytes, &pfor_slots, true);
    let t_exc_p = time_decode(budget, short_ints, &dec_exc, pfor_bytes, &pfor_slots, true);
    let t_clean_b = time_decode(budget, short_ints, &dec_clean, &bp.bytes, &bp_slots, false);
    let t_exc_b = time_decode(budget, short_ints, &dec_exc, &bp.bytes, &bp_slots, false);
    println!("decode fresh clean n={}: pfor {}  bp128 {}", dec_clean.len(), fmt_timed(t_clean_p), fmt_timed(t_clean_b));
    println!("decode fresh exc   n={}: pfor {}  bp128 {}", dec_exc.len(), fmt_timed(t_exc_p), fmt_timed(t_exc_b));
    let t_scan = time_it(budget, short_ints, || {
        for s in &scans {
            black_box(scalar_hit(&s.block, s.target, s.start));
        }
    });
    let t_simd = time_it(budget, short_ints, || {
        for s in &scans {
            black_box(simd_hit(&s.block, s.target, s.start));
        }
    });
    let t_scan_u = time_it(budget, short_ints, || {
        for s in &scans_unary {
            black_box(scalar_hit(&s.block, s.target, s.start));
        }
    });
    let t_bit = time_it(budget, short_ints, || {
        for b in &bits {
            black_box(next_set_bit(&b.words, b.num_longs, b.from));
        }
    });
    println!("scalar scan all lands n={}: {}", scans.len(), fmt_timed(t_scan));
    println!("sse4 scan   all lands n={}: {}", scans.len(), fmt_timed(t_simd));
    if avx512 {
        let t_avx = time_it(budget, short_ints, || {
            for s in &scans {
                // SAFETY: `avx512` is the `avx512f` feature check above.
                black_box(unsafe { simd_hit_avx512(&s.block, s.target, s.start) });
            }
        });
        println!("avx512 scan all lands n={}: {}", scans.len(), fmt_timed(t_avx));
    }
    println!("scalar scan unary     n={}: {}", scans_unary.len(), fmt_timed(t_scan_u));
    println!("next_set_bit unary    n={}: {}", bits.len(), fmt_timed(t_bit));

    let t_bin = time_it(budget, short_ints, || {
        for q in &searches {
            let buf = slot_buf(pfor_bytes, &pfor_slots[q.slot]);
            black_box(binary_search(buf, q.from, q.blocks, q.target));
        }
    });
    let t_gal = time_it(budget, short_ints, || {
        for q in &searches {
            let buf = slot_buf(pfor_bytes, &pfor_slots[q.slot]);
            black_box(table_find(buf, q.from, q.blocks, q.target));
        }
    });
    println!("binary search n={}: {}", searches.len(), fmt_timed(t_bin));
    println!("table find    n={}: {}", searches.len(), fmt_timed(t_gal));
    Ok(())
}

fn scan_clone(s: &ScanItem) -> ScanItem {
    ScanItem { block: s.block, target: s.target, start: s.start, hit: s.hit }
}

fn bit_item(list: &[u32], c: &Call, token: i8) -> Result<BitItem> {
    let start = c.block * BLOCK;
    let level0_last = if c.block == 0 { -1i64 } else { i64::from(list[start - 1]) };
    let base = level0_last + 1;
    let mut words = [0u64; 64];
    let num_longs = if token == 0 {
        words[0] = u64::MAX;
        words[1] = u64::MAX;
        2
    } else if token < 0 {
        let n = usize::from(token.unsigned_abs());
        for &doc in &list[start..start + BLOCK] {
            let idx = (i64::from(doc) - base) as usize;
            if idx / 64 >= n {
                bail!("bitset index {idx} outside {n} longs");
            }
            words[idx / 64] |= 1u64 << (idx % 64);
        }
        n
    } else {
        bail!("bit_item on packed token");
    };
    let Some(expect) = c.y else { bail!("bitset landing has no value") };
    Ok(BitItem { words, num_longs, base, from: i64::from(c.target) - base, expect })
}

fn slots_for(enc: &Encoded, lists: &[usize]) -> Vec<Slot> {
    lists
        .iter()
        .map(|&i| {
            let (off, len, n) = enc.span[i].unwrap_or((0, 0, 0));
            Slot { n, off, len }
        })
        .collect()
}

fn slot_buf<'a>(bytes: &'a [u8], s: &Slot) -> &'a [u8] {
    &bytes[s.off..s.off + s.len]
}

fn time_decode(
    budget: Duration,
    short_ints: usize,
    items: &[DecItem],
    bytes: &[u8],
    slots: &[Slot],
    pfor: bool,
) -> Timed {
    time_it(budget, short_ints, || {
        for it in items {
            let s = &slots[it.slot];
            let buf = &bytes[s.off..s.off + s.len];
            if pfor {
                black_box(intpack::pfor128skip::get_sorted(s.n, buf, it.index));
            } else {
                black_box(Bp128Skip.get(Kind::Sorted, 0, s.n, buf, it.index));
            }
        }
    })
}

fn table_last(buf: &[u8], b: usize) -> u32 {
    let i = b * 8;
    u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]])
}

fn binary_search(buf: &[u8], from: usize, blocks: usize, target: u32) -> usize {
    let (mut lo, mut hi) = (from, blocks);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if table_last(buf, mid) < target { lo = mid + 1 } else { hi = mid }
    }
    lo
}

/// `Table::find`: current block, binary search from block 0, otherwise a
/// gallop of 1, 2, 4, … blocks ahead of `from`.
fn table_find(buf: &[u8], from: usize, blocks: usize, target: u32) -> usize {
    if from >= blocks {
        return blocks;
    }
    if table_last(buf, from) >= target {
        return from;
    }
    if from == 0 {
        return binary_search(buf, 1, blocks, target);
    }
    let mut prev = from;
    let mut step = 1usize;
    loop {
        let probe = from.saturating_add(step);
        if probe >= blocks || table_last(buf, probe) >= target {
            let hi = if probe >= blocks { blocks } else { probe + 1 };
            return binary_search(buf, prev + 1, hi, target);
        }
        prev = probe;
        step = step.saturating_mul(2);
    }
}

fn scalar_hit(block: &[u32; BLOCK], target: u32, start: usize) -> usize {
    let mut j = start;
    while j < BLOCK {
        if block[j] >= target {
            return j;
        }
        j += 1;
    }
    BLOCK
}

fn simd_hit(block: &[u32; BLOCK], target: u32, start: usize) -> usize {
    // SAFETY: x86_64 mandates SSE2, which this kernel uses.
    unsafe { simd_hit_sse(block, target, start) }
}

#[target_feature(enable = "sse2")]
unsafe fn simd_hit_sse(block: &[u32; BLOCK], target: u32, start: usize) -> usize {
    use std::arch::x86_64::{
        _mm_castsi128_ps, _mm_cmpgt_epi32, _mm_loadu_si128, _mm_movemask_ps, _mm_set1_epi32, _mm_xor_si128,
    };
    // SAFETY: caller has sse2. Each load is a 16-byte group inside `block`.
    // Same kernel as `pfor128::scan_geq`: align down to 4 lanes and skip the
    // lanes before `start`.
    unsafe {
        let bias = _mm_set1_epi32(i32::MIN);
        let needle = _mm_xor_si128(_mm_set1_epi32(target as i32), bias);
        let mut i = start & !3;
        let mut skip = (start - i) as u32;
        while i < BLOCK {
            let below = _mm_cmpgt_epi32(needle, _mm_xor_si128(_mm_loadu_si128(block.as_ptr().add(i).cast()), bias));
            let hits = (((!_mm_movemask_ps(_mm_castsi128_ps(below))) & 0b1111) as u32 >> skip) << skip;
            if hits != 0 {
                return i + hits.trailing_zeros() as usize;
            }
            i += 4;
            skip = 0;
        }
        BLOCK
    }
}

#[target_feature(enable = "avx512f")]
unsafe fn simd_hit_avx512(block: &[u32; BLOCK], target: u32, start: usize) -> usize {
    use std::arch::x86_64::{_mm512_cmpgt_epi32_mask, _mm512_loadu_si512, _mm512_set1_epi32, _mm512_xor_si512};
    // SAFETY: caller has avx512f. Each load is a 64-byte group inside `block`
    // (`i` stays on a 16-element boundary and stops at `BLOCK`).
    unsafe {
        let bias = _mm512_set1_epi32(i32::MIN);
        let needle = _mm512_xor_si512(_mm512_set1_epi32(target as i32), bias);
        let mut i = start & !15;
        let mut skip = (start - i) as u32;
        while i < BLOCK {
            let below = _mm512_cmpgt_epi32_mask(
                needle,
                _mm512_xor_si512(_mm512_loadu_si512(block.as_ptr().add(i).cast()), bias),
            );
            let hits = ((!below) >> skip) << skip;
            if hits != 0 {
                return i + hits.trailing_zeros() as usize;
            }
            i += 16;
            skip = 0;
        }
        BLOCK
    }
}

fn next_set_bit(words: &[u64; 64], num_longs: usize, from: i64) -> Option<i64> {
    if num_longs == 0 {
        return None;
    }
    let from = from.max(0) as usize;
    let mut wi = from / 64;
    if wi >= num_longs {
        return None;
    }
    let mut word = words[wi] & (u64::MAX << (from % 64));
    loop {
        if word != 0 {
            return Some((wi * 64 + word.trailing_zeros() as usize) as i64);
        }
        wi += 1;
        if wi >= num_longs {
            return None;
        }
        word = words[wi];
    }
}
