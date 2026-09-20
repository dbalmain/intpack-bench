//! `bitpacking` crate's `BitPacker4x`: SIMD bitpacking in 128-value blocks,
//! with a VByte tail for the sub-block remainder — the Lucene/Tantivy shape
//! that the Zipfian datasets exercise. No skip data (block maxima, offsets):
//! this variant measures plain bitpacking, and the cursor decodes blocks one
//! by one and scans. Sorted lists use the crate's strictly-sorted delta mode
//! (`gap - 1`); the first block's initial is `None`, later blocks carry the
//! previous block's last value.

use crate::stream::Kind;

use bitpacking::{BitPacker, BitPacker4x};

use super::vbyte;
use super::{Caps, Codec, Cursor};

const BLOCK_LEN: usize = BitPacker4x::BLOCK_LEN;

pub struct Bp128;

impl Codec for Bp128 {
    fn name(&self) -> &'static str {
        "bp128"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: false, streaming_encoder: true, seek: true, random_access: false }
    }

    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        let bp = BitPacker4x::new();
        match kind {
            Kind::Sorted => {
                // Every full 128-block: one num_bits byte, then the packed bits.
                let mut initial: Option<u32> = None;
                for block in list.chunks_exact(BLOCK_LEN) {
                    let nb = bp.num_bits_strictly_sorted(initial, block);
                    let size = BitPacker4x::compressed_block_size(nb);
                    let start = out.len();
                    out.push(nb);
                    out.resize(start + 1 + size, 0);
                    bp.compress_strictly_sorted(initial, block, &mut out[start + 1..], nb);
                    initial = Some(block[BLOCK_LEN - 1]);
                }
                // The remainder goes through VByte, delta-coded like vbyte.rs:
                // first value as-is, then gap - 1.
                let mut prev = initial.unwrap_or(0);
                for (i, &v) in list.chunks_exact(BLOCK_LEN).remainder().iter().enumerate() {
                    let gap = if i == 0 && initial.is_none() { v } else { v - prev - 1 };
                    vbyte::put(gap, out);
                    prev = v;
                }
            }
            Kind::Unsorted => {
                for block in list.chunks_exact(BLOCK_LEN) {
                    let nb = bp.num_bits(block);
                    let size = BitPacker4x::compressed_block_size(nb);
                    let start = out.len();
                    out.push(nb);
                    out.resize(start + 1 + size, 0);
                    bp.compress(block, &mut out[start + 1..], nb);
                }
                for &v in list.chunks_exact(BLOCK_LEN).remainder() {
                    vbyte::put(v, out);
                }
            }
        }
    }

    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        let bp = BitPacker4x::new();
        let mut block = [0u32; BLOCK_LEN];
        match kind {
            Kind::Sorted => {
                let mut byte = 0usize;
                let mut initial: Option<u32> = None;
                for _ in 0..n / BLOCK_LEN {
                    let nb = buf[byte];
                    byte += 1;
                    let size = BitPacker4x::compressed_block_size(nb);
                    let _ = bp.decompress_strictly_sorted(initial, &buf[byte..byte + size], &mut block, nb);
                    byte += size;
                    out.extend_from_slice(&block);
                    initial = Some(block[BLOCK_LEN - 1]);
                }
                let mut prev = initial.unwrap_or(0);
                for i in 0..n % BLOCK_LEN {
                    let gap = vbyte::take(buf, &mut byte);
                    let v = if i == 0 && initial.is_none() { gap } else { prev + gap + 1 };
                    out.push(v);
                    prev = v;
                }
            }
            Kind::Unsorted => {
                let mut byte = 0usize;
                for _ in 0..n / BLOCK_LEN {
                    let nb = buf[byte];
                    byte += 1;
                    let size = BitPacker4x::compressed_block_size(nb);
                    let _ = bp.decompress(&buf[byte..byte + size], &mut block, nb);
                    byte += size;
                    out.extend_from_slice(&block);
                }
                for _ in 0..n % BLOCK_LEN {
                    out.push(vbyte::take(buf, &mut byte));
                }
            }
        }
    }

    fn cursor<'a>(&self, _universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(Bp128Cursor {
            bp: BitPacker4x::new(),
            buf,
            n,
            full: n / BLOCK_LEN * BLOCK_LEN,
            byte: 0,
            block: [0; BLOCK_LEN],
            block_num: usize::MAX,
            idx: usize::MAX,
            cur: None,
            tail_r: 0,
            tail_last: 0,
        }))
    }
}

/// Lazy block-by-block cursor: decodes a 128-value block when the scan first
/// enters it, keeps the block hot, and falls through to the VByte tail. There
/// is deliberately no skip data, so a seek to the far end decodes every block
/// on the way — that is the cost this codec is measured without.
struct Bp128Cursor<'a> {
    bp: BitPacker4x,
    buf: &'a [u8],
    n: usize,
    full: usize,
    /// Byte offset of the next full block's num_bits byte (also where the
    /// VByte tail starts once the full blocks are exhausted).
    byte: usize,
    block: [u32; BLOCK_LEN],
    block_num: usize,
    idx: usize,
    cur: Option<u32>,
    tail_r: usize,
    tail_last: u32,
}

impl Bp128Cursor<'_> {
    fn load_block(&mut self, b: usize) {
        let nb = self.buf[self.byte];
        self.byte += 1;
        let size = BitPacker4x::compressed_block_size(nb);
        let initial = (b > 0).then(|| self.block[BLOCK_LEN - 1]);
        let _ =
            self.bp.decompress_strictly_sorted(initial, &self.buf[self.byte..self.byte + size], &mut self.block, nb);
        self.byte += size;
        self.block_num = b;
    }

    fn value_at(&mut self, i: usize) -> u32 {
        if i < self.full {
            let b = i / BLOCK_LEN;
            if b != self.block_num {
                self.load_block(b);
            }
            self.block[i % BLOCK_LEN]
        } else {
            while self.tail_r <= i - self.full {
                self.next_tail();
            }
            self.tail_last
        }
    }

    /// Decode the next tail value. `tail_last` is seeded from the previous
    /// block only when a full block exists — the tail's first gap is then
    /// `v - prev - 1`, otherwise the first gap is the value itself.
    fn next_tail(&mut self) {
        if self.tail_r == 0 && self.full > 0 {
            self.tail_last = self.block[BLOCK_LEN - 1];
        }
        let gap = vbyte::take(self.buf, &mut self.byte);
        self.tail_r += 1;
        self.tail_last = if self.full > 0 || self.tail_r > 1 { self.tail_last + gap + 1 } else { gap };
    }
}

impl Cursor for Bp128Cursor<'_> {
    fn next_geq(&mut self, target: u32) -> Option<u32> {
        if let Some(c) = self.cur
            && c >= target
        {
            return Some(c);
        }
        while let Some(v) = self.next() {
            if v >= target {
                return Some(v);
            }
        }
        None
    }

    fn next(&mut self) -> Option<u32> {
        self.idx = self.idx.wrapping_add(1);
        self.cur = (self.idx < self.n).then(|| self.value_at(self.idx));
        self.cur
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::Bp128);
    }
}
