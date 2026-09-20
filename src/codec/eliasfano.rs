//! `sucds` crate's `EliasFano`, built for the list's universe and count,
//! `enable_rank`'d so successor/rank work, and serialised with the crate's
//! `Serializable` trait. `open` deserialises once; the cursor keeps the
//! current index and jumps with `rank(target)` (the first index whose value
//! is `>= target`), and `get` is `select`. An empty list cannot be built
//! (`EliasFanoBuilder::new` rejects `num_vals == 0`), so it is encoded as an
//! EF with capacity 1 and nothing pushed — an EF with zero ones.

use crate::stream::Kind;

use sucds::Serializable;
use sucds::bit_vectors::DArray;
use sucds::mii_sequences::{EliasFano, EliasFanoBuilder};

use super::{Caps, Codec, Cursor, Prepared};

pub struct EliasFanoCodec;

impl Codec for EliasFanoCodec {
    fn name(&self) -> &'static str {
        "elias-fano"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: true, streaming_encoder: true, seek: true, random_access: true }
    }

    fn encode(&self, _kind: Kind, universe: u32, list: &[u32], out: &mut Vec<u8>) {
        let Ok(mut builder) = EliasFanoBuilder::new(u64::from(universe), list.len().max(1)) else {
            return; // `new` rejects only `num_vals == 0`, which `max(1)` rules out
        };
        let _ = builder.extend(list.iter().map(|&v| u64::from(v)));
        let ef = builder.build().enable_rank();
        let _ = ef.serialize_into(&mut *out);
    }

    fn decode(&self, _kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        let Ok(ef) = EliasFano::deserialize_from(buf) else {
            return;
        };
        if n > 0 {
            // `iter(0)` debug-asserts the sequence is non-empty.
            for v in ef.iter(0).take(n) {
                out.push(v as u32);
            }
        }
    }

    fn aux_bytes(&self, _universe: u32, _n: usize, buf: &[u8]) -> Option<usize> {
        // Payload is the high/low bit vectors; the DArray select indices
        // (select1, plus the select0 index `enable_rank` adds) are the
        // index overhead. Re-parse the leading DArray to measure them.
        let Ok(high) = DArray::deserialize_from(buf) else {
            return None;
        };
        let mut aux = high.s1_index().size_in_bytes();
        if let Some(s0) = high.s0_index() {
            aux += s0.size_in_bytes();
        }
        Some(aux)
    }

    fn cursor<'a>(&self, _universe: u32, _n: usize, _buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        // `open` overrides this with an owning deserialisation.
        None
    }

    fn open<'a>(&self, _kind: Kind, _universe: u32, _n: usize, buf: &'a [u8]) -> Option<Box<dyn Prepared + 'a>> {
        let Ok(ef) = EliasFano::deserialize_from(buf) else {
            return None;
        };
        Some(Box::new(EfPrepared { ef }))
    }
}

struct EfPrepared {
    ef: EliasFano,
}

impl Prepared for EfPrepared {
    fn cursor(&self) -> Option<Box<dyn Cursor + '_>> {
        Some(Box::new(EfCursor { ef: &self.ef, k: usize::MAX, cur: None }))
    }

    fn get(&self, i: usize) -> Option<u32> {
        self.ef.select(i).map(|v| v as u32)
    }
}

/// `k` is the index of the current element; `usize::MAX` is "before the
/// first". `next_geq` jumps straight to `rank(target)` — the crate's rank
/// counts elements below `target`, which is exactly the successor index.
struct EfCursor<'a> {
    ef: &'a EliasFano,
    k: usize,
    cur: Option<u32>,
}

impl Cursor for EfCursor<'_> {
    fn next_geq(&mut self, target: u32) -> Option<u32> {
        if let Some(c) = self.cur
            && c >= target
        {
            return Some(c);
        }
        self.k = self.ef.rank(u64::from(target)).unwrap_or(self.ef.len());
        self.cur = self.ef.select(self.k).map(|v| v as u32);
        self.cur
    }

    fn next(&mut self) -> Option<u32> {
        self.k = self.k.wrapping_add(1);
        self.cur = self.ef.select(self.k).map(|v| v as u32);
        self.cur
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::EliasFanoCodec);
    }
}
