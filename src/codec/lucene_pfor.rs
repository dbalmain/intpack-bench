//! Lucene 10.3.1 `PForUtil` over 128-value blocks: the way Lucene stores freqs
//! and positions (and the way Lucene ≤ 9 stored doc deltas). Full blocks go
//! through [`super::lucene::pfor`]; the trailing `n % 128` values are plain
//! Lucene vints. Sorted lists are delta-coded with `prev` starting at −1 so
//! every delta is ≥ 1. No header, no skip data: `n` and `universe` arrive out
//! of band. Full blocks outside Lucene's signed-positive domain use the raw
//! escape documented by [`super::lucene::pfor`].

use crate::stream::Kind;

use super::lucene::BLOCK_SIZE;
use super::lucene::io::{Reader, write_vint};
use super::lucene::pfor;
use super::{Caps, Codec};

pub struct LucenePFor;

impl Codec for LucenePFor {
    fn name(&self) -> &'static str {
        "lucene-pfor"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: false, streaming_encoder: true, seek: false, random_access: false }
    }

    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        let (blocks, tail) = list.as_chunks::<BLOCK_SIZE>();
        match kind {
            Kind::Sorted => {
                let mut prev = u32::MAX;
                let mut deltas = [0u32; BLOCK_SIZE];
                for block in blocks {
                    for (i, &v) in block.iter().enumerate() {
                        deltas[i] = v.wrapping_sub(prev);
                        prev = v;
                    }
                    pfor::encode(&deltas, out);
                }
                for &v in tail {
                    write_vint(out, v.wrapping_sub(prev));
                    prev = v;
                }
            }
            Kind::Unsorted => {
                for block in blocks {
                    pfor::encode(block, out);
                }
                for &v in tail {
                    write_vint(out, v);
                }
            }
        }
    }

    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        let mut r = Reader::new(buf);
        let n_full = n / BLOCK_SIZE;
        let mut block = [0u32; BLOCK_SIZE];
        match kind {
            Kind::Sorted => {
                let mut prev = u32::MAX;
                for _ in 0..n_full {
                    let consumed = pfor::decode(r.buf.get(r.pos..).unwrap_or(&[]), &mut block);
                    r.skip(consumed);
                    for &d in &block {
                        prev = prev.wrapping_add(d);
                        out.push(prev);
                    }
                }
                for _ in 0..(n % BLOCK_SIZE) {
                    prev = prev.wrapping_add(r.read_vint());
                    out.push(prev);
                }
            }
            Kind::Unsorted => {
                for _ in 0..n_full {
                    let consumed = pfor::decode(r.buf.get(r.pos..).unwrap_or(&[]), &mut block);
                    r.skip(consumed);
                    out.extend_from_slice(&block);
                }
                for _ in 0..(n % BLOCK_SIZE) {
                    out.push(r.read_vint());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::Kind;

    #[test]
    fn conformance() {
        super::super::conformance(&LucenePFor);
    }

    #[test]
    fn sorted_block_is_pfor_of_deltas() {
        let list: Vec<u32> = (0..128).map(|i| i * 3 + 1).collect();
        let mut deltas = [0u32; BLOCK_SIZE];
        let mut prev = u32::MAX;
        for (i, &v) in list.iter().enumerate() {
            deltas[i] = v.wrapping_sub(prev);
            prev = v;
        }
        let mut expected = Vec::new();
        pfor::encode(&deltas, &mut expected);
        let mut got = Vec::new();
        LucenePFor.encode(Kind::Sorted, 1 << 20, &list, &mut got);
        assert_eq!(got, expected);
    }

    #[test]
    fn full_block_round_trips_u32_values() {
        let raw: Vec<u32> =
            (0..BLOCK_SIZE).map(|i| if i.is_multiple_of(3) { u32::MAX - i as u32 } else { i as u32 }).collect();
        let mut bytes = Vec::new();
        LucenePFor.encode(Kind::Unsorted, u32::MAX, &raw, &mut bytes);
        assert_eq!(bytes[0], u8::MAX);
        assert_eq!(pfor::skip(&bytes), bytes.len());

        let mut patched = vec![0; BLOCK_SIZE];
        patched[57] = u32::MAX;
        let mut bytes = Vec::new();
        LucenePFor.encode(Kind::Unsorted, u32::MAX, &patched, &mut bytes);
        assert_ne!(bytes[0], u8::MAX);
        assert_eq!(pfor::skip(&bytes), bytes.len());

        for list in [&raw, &patched] {
            if let Err(e) = super::super::check(&LucenePFor, Kind::Unsorted, u32::MAX, list) {
                panic!("{e}");
            }
        }
    }
}
