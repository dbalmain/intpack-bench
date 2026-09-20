//! intpack's `elias_fano` codec: plain Elias-Fano with a sampled select
//! index, so a fresh seek jumps straight to the target region rather than
//! walking the highs. `aux_bytes` reports the select samples — 4 bytes per
//! 256 elements, against sucds's ~1.3 bits/int of select1/select0 indices.

use crate::stream::Kind;

use intpack::Cursor as IpCursor;
use intpack::elias_fano;

use super::{Caps, Codec, Cursor};

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

    fn aux_bytes(&self, n: usize, _buf: &[u8]) -> Option<usize> {
        Some(elias_fano::aux_len(0, n))
    }

    fn cursor<'a>(&self, universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(EfCursor { cur: elias_fano::Cursor::new(universe, n, buf) }))
    }

    fn get(&self, _kind: Kind, universe: u32, n: usize, buf: &[u8], i: usize) -> Option<u32> {
        (i < n).then(|| elias_fano::get(universe, n, buf, i))
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
    use crate::codec::{self, Codec as _, Codec, Prepared};
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

    #[test]
    fn repro_uniform_lists() {
        let ds = crate::stream::Dataset::read(std::path::Path::new("data/synthetic/uniform-d0.1.ipb")).expect("read dataset");
        let universe = ds.meta.universe;
        let codec = &super::IntpackEf;
        let mut arena = Vec::new();
        let mut spans = Vec::new();
        for list in &ds.lists {
            let start = arena.len();
            codec.encode(Kind::Sorted, universe, list, &mut arena);
            spans.push((start, arena.len() - start));
        }
        let prepared: Vec<Box<dyn Prepared>> = ds
            .lists
            .iter()
            .zip(&spans)
            .map(|(l, &(o, n))| codec::prepare(codec, Kind::Sorted, universe, l.len(), &arena[o..o + n]))
            .collect();
        for &(s, l) in &[(5usize, 17usize), (0, 20), (3, 12), (10, 30)] {
            let got = leapfrog(prepared[s].as_ref(), prepared[l].as_ref());
            let want = naive_count(&ds.lists[s], &ds.lists[l]);
            eprintln!("lists {s}x{l}: got {got} vs naive {want}");
            assert_eq!(got, want, "lists {s}x{l}: {got} vs naive {want}");
        }
    }
}
