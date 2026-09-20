//! intpack's `elias_fano` codec: plain Elias-Fano with a sampled select
//! index, so a fresh seek jumps straight to the target region rather than
//! walking the highs. Short highs omit the index; long highs use two-level
//! select-one and select-zero samples.

use crate::stream::Kind;

use intpack::Cursor as IpCursor;
use intpack::elias_fano;

use super::{Caps, Codec, Cursor, Prepared};

pub struct IntpackEf;

impl Codec for IntpackEf {
    fn name(&self) -> &'static str {
        "ip-ef"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: true, streaming_encoder: false, seek: true, random_access: true }
    }

    fn encode(&self, _kind: Kind, universe: u32, list: &[u32], out: &mut Vec<u8>) {
        elias_fano::encode(universe, list, out);
    }

    fn decode(&self, _kind: Kind, universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        elias_fano::decode(universe, n, buf, out);
    }

    fn aux_bytes(&self, universe: u32, n: usize, _buf: &[u8]) -> Option<usize> {
        Some(elias_fano::aux_len(universe, n))
    }

    fn cursor<'a>(&self, universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(EfCursor { cur: elias_fano::Cursor::new(universe, n, buf) }))
    }

    fn get(&self, _kind: Kind, universe: u32, n: usize, buf: &[u8], i: usize) -> Option<u32> {
        (i < n).then(|| elias_fano::get(universe, n, buf, i))
    }

    fn open<'a>(&self, _kind: Kind, universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Prepared + 'a>> {
        Some(Box::new(EfPrepared { view: elias_fano::View::new(universe, n, buf), n }))
    }
}

struct EfPrepared<'a> {
    view: elias_fano::View<'a>,
    n: usize,
}

impl Prepared for EfPrepared<'_> {
    fn cursor(&self) -> Option<Box<dyn Cursor + '_>> {
        Some(Box::new(EfCursor { cur: self.view.cursor() }))
    }

    fn get(&self, i: usize) -> Option<u32> {
        (i < self.n).then(|| self.view.get(i))
    }
}

struct EfCursor<'a> {
    cur: elias_fano::Cursor<'a>,
}

impl Cursor for EfCursor<'_> {
    fn next_geq(&mut self, target: u32) -> Option<u32> {
        IpCursor::next_geq(&mut self.cur, target)
    }

    fn next(&mut self) -> Option<u32> {
        IpCursor::next(&mut self.cur)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::IntpackEf);
    }
}

#[cfg(test)]
mod ip_repro {
    use crate::codec::{self, Codec as _, Prepared};
    use crate::stream::Kind;

    fn naive_count(a: &[u32], b: &[u32]) -> usize {
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

    fn leapfrog(a: &dyn Prepared, b: &dyn Prepared) -> usize {
        let (Some(mut a), Some(mut b)) = (a.cursor(), b.cursor()) else {
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
    }

    /// Encode several dense Bernoulli lists into one arena and leapfrog
    /// every pair — exercises both the arena-append path and next_geq on
    /// running cursors (the bench's intersect correctness check).
    #[test]
    fn leapfrog_matches_naive_on_dense_lists() {
        let universe = 1 << 20;
        let codec = &super::IntpackEf;
        let mut arena = Vec::new();
        let mut spans = Vec::new();
        let mut lists = Vec::new();
        let mut state = 0x1234_5678_9abc_def0u64;
        for round in 0..8 {
            let mut list = Vec::new();
            for v in 0..universe {
                state ^= state << 7;
                state ^= state >> 9;
                state ^= state << 8;
                if state.is_multiple_of(10) && (state >> 32).is_multiple_of(round + 3) {
                    list.push(v);
                }
            }
            let start = arena.len();
            codec.encode(Kind::Sorted, universe, &list, &mut arena);
            spans.push((start, arena.len() - start));
            lists.push(list);
        }
        let prepared: Vec<Box<dyn Prepared>> = lists
            .iter()
            .zip(&spans)
            .map(|(l, &(o, n))| codec::prepare(codec, Kind::Sorted, universe, l.len(), &arena[o..o + n]))
            .collect();
        for s in 0..lists.len() {
            for l in 0..lists.len() {
                if s == l {
                    continue;
                }
                let got = leapfrog(prepared[s].as_ref(), prepared[l].as_ref());
                let want = naive_count(&lists[s], &lists[l]);
                assert_eq!(got, want, "lists {s}x{l}: {got} vs naive {want}");
            }
        }
    }
}
