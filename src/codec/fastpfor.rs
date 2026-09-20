//! `fastpfor` crate's pure-Rust `FastPFor128` (128-value patched frame-of-reference
//! blocks, `VariableByte` tail — the `CompositeCodec`). Sorted lists are
//! delta-coded to gaps (`gap - 1`, first value as-is, the vbyte convention)
//! before being handed to the crate. The crate writes its own block-count
//! header word (4 bytes per list, including for the empty list); it is left
//! intact. Words are stored little-endian.

use crate::stream::Kind;

use fastpfor::{AnyLenCodec, FastPFor128};

use super::{Caps, Codec};

pub struct FastPFor;

impl Codec for FastPFor {
    fn name(&self) -> &'static str {
        "fastpfor128"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: false, streaming_encoder: true, seek: false, random_access: false }
    }

    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        let gaps: Vec<u32> = match kind {
            Kind::Sorted => {
                let mut prev = 0u32;
                list.iter()
                    .enumerate()
                    .map(|(i, &v)| {
                        let gap = if i == 0 { v } else { v - prev - 1 };
                        prev = v;
                        gap
                    })
                    .collect()
            }
            Kind::Unsorted => list.to_vec(),
        };
        let mut words = Vec::new();
        let mut codec = FastPFor128::default();
        let _ = codec.encode(&gaps, &mut words);
        out.reserve(words.len() * 4);
        for w in words {
            out.extend_from_slice(&w.to_le_bytes());
        }
    }

    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        let words: Vec<u32> = buf.as_chunks::<4>().0.iter().map(|&c| u32::from_le_bytes(c)).collect();
        let mut gaps = Vec::new();
        let mut codec = FastPFor128::default();
        let _ = codec.decode(&words, &mut gaps, Some(n as u32));
        match kind {
            Kind::Sorted => {
                let mut prev = 0u32;
                for (i, &gap) in gaps.iter().enumerate() {
                    let v = if i == 0 { gap } else { prev + gap + 1 };
                    out.push(v);
                    prev = v;
                }
            }
            Kind::Unsorted => out.extend_from_slice(&gaps),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::FastPFor);
    }
}
