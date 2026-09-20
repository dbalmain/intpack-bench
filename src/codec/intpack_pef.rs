//! intpack's `pef` codec: fixed-128 partitioned Elias-Fano whose upper level
//! carries the select samples, so a fresh seek jumps to the target partition
//! instead of the reference `pef`'s binary search with a linear select each
//! probe. `aux_bytes` reports header + directory + upper select samples.

use crate::stream::Kind;

use intpack::Cursor as IpCursor;
use intpack::pef;

use super::{Caps, Codec, Cursor};

pub struct IntpackPef;

impl Codec for IntpackPef {
    fn name(&self) -> &'static str {
        "ip-pef"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: true, streaming_encoder: false, seek: true, random_access: true }
    }

    fn encode(&self, _kind: Kind, universe: u32, list: &[u32], out: &mut Vec<u8>) {
        pef::encode(universe, list, out);
    }

    fn decode(&self, _kind: Kind, universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        pef::decode(universe, n, buf, out);
    }

    fn aux_bytes(&self, universe: u32, n: usize, buf: &[u8]) -> Option<usize> {
        Some(pef::aux_bytes(universe, n, buf))
    }

    fn cursor<'a>(&self, universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(PefCursor { cur: pef::Cursor::new(universe, n, buf) }))
    }

    fn get(&self, _kind: Kind, universe: u32, n: usize, buf: &[u8], i: usize) -> Option<u32> {
        (i < n).then(|| pef::get(universe, n, buf, i))
    }
}

struct PefCursor<'a> {
    cur: pef::Cursor<'a>,
}

impl Cursor for PefCursor<'_> {
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
        super::super::conformance(&super::IntpackPef);
    }
}

#[cfg(test)]
mod pef_repro {
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

    /// Arena-append encode + leapfrog on dense/clustered lists, matching the
    /// bench's intersect path.
    #[test]
    fn leapfrog_matches_naive_on_dense_lists() {
        let universe = 1 << 20;
        let codec = &super::IntpackPef;
        let mut arena = Vec::new();
        let mut spans = Vec::new();
        let mut lists = Vec::new();
        let mut state = 0xdead_beef_1234_5678u64;
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
