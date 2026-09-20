//! `bitpacking` crate's `BitPacker4x`: SIMD bitpacking in 128-value blocks,
//! with a VByte tail for the sub-block remainder — the Lucene/Tantivy shape
//! that the Zipfian datasets exercise. No skip data (block maxima, offsets):
//! this variant measures plain bitpacking, and the cursor decodes blocks one
//! by one and scans. Sorted lists use the crate's strictly-sorted delta mode
//! (`gap - 1`); the first block's initial is `None`, later blocks carry the
//! previous block's last value.
//!
//! The block and tail encode/decode helpers and the tail cursor state are
//! `pub(super)`: `bp128-skip` shares them and adds a skip table on top.

use crate::stream::Kind;

use bitpacking::{BitPacker, BitPacker4x};

use super::vbyte;
use super::{Caps, Codec, Cursor};

pub(super) const BLOCK_LEN: usize = BitPacker4x::BLOCK_LEN;

// ── shared with bp128-skip ──

/// Encode one full 128-block of a strictly-sorted list: a `num_bits` byte,
/// then the packed bits. Returns the block's final value (the next block's
/// delta base).
pub(super) fn encode_block_sorted(bp: &BitPacker4x, initial: Option<u32>, block: &[u32], out: &mut Vec<u8>) -> u32 {
    let nb = bp.num_bits_strictly_sorted(initial, block);
    let size = BitPacker4x::compressed_block_size(nb);
    let start = out.len();
    out.push(nb);
    out.resize(start + 1 + size, 0);
    bp.compress_strictly_sorted(initial, block, &mut out[start + 1..], nb);
    block[BLOCK_LEN - 1]
}

/// Encode one full 128-block of an unsorted list: a `num_bits` byte, then the
/// packed bits.
pub(super) fn encode_block_unsorted(bp: &BitPacker4x, block: &[u32], out: &mut Vec<u8>) {
    let nb = bp.num_bits(block);
    let size = BitPacker4x::compressed_block_size(nb);
    let start = out.len();
    out.push(nb);
    out.resize(start + 1 + size, 0);
    bp.compress(block, &mut out[start + 1..], nb);
}

/// Decode one full 128-block whose `num_bits` byte is at `buf[*byte]`,
/// advancing `*byte` past the block, into `out`. `initial` is the previous
/// block's final value (`None` for the first). Returns the block's final
/// value.
pub(super) fn decode_block_sorted(
    bp: &BitPacker4x,
    initial: Option<u32>,
    buf: &[u8],
    byte: &mut usize,
    out: &mut [u32; BLOCK_LEN],
) -> u32 {
    let nb = buf[*byte];
    *byte += 1;
    let size = BitPacker4x::compressed_block_size(nb);
    let _ = bp.decompress_strictly_sorted(initial, &buf[*byte..*byte + size], out, nb);
    *byte += size;
    out[BLOCK_LEN - 1]
}

/// Decode one full 128-block of an unsorted list whose `num_bits` byte is at
/// `buf[*byte]`, advancing `*byte` past the block, into `out`.
pub(super) fn decode_block_unsorted(bp: &BitPacker4x, buf: &[u8], byte: &mut usize, out: &mut [u32; BLOCK_LEN]) {
    let nb = buf[*byte];
    *byte += 1;
    let size = BitPacker4x::compressed_block_size(nb);
    let _ = bp.decompress(&buf[*byte..*byte + size], out, nb);
    *byte += size;
}

/// Encode the sorted VByte tail: first value as-is (or relative to the last
/// full block's last value, carried in `initial`), then `gap - 1`.
pub(super) fn encode_tail_sorted(initial: Option<u32>, tail: &[u32], out: &mut Vec<u8>) {
    let mut prev = initial.unwrap_or(0);
    for (i, &v) in tail.iter().enumerate() {
        let gap = if i == 0 && initial.is_none() { v } else { v - prev - 1 };
        vbyte::put(gap, out);
        prev = v;
    }
}

/// Encode the unsorted VByte tail: each value as-is.
pub(super) fn encode_tail_unsorted(tail: &[u32], out: &mut Vec<u8>) {
    for &v in tail {
        vbyte::put(v, out);
    }
}

/// Decode `n` sorted tail values from `buf` at `*byte`, appending to `out`.
pub(super) fn decode_tail_sorted(initial: Option<u32>, buf: &[u8], byte: &mut usize, n: usize, out: &mut Vec<u32>) {
    let mut prev = initial.unwrap_or(0);
    for i in 0..n {
        let gap = vbyte::take(buf, byte);
        let v = if i == 0 && initial.is_none() { gap } else { prev + gap + 1 };
        out.push(v);
        prev = v;
    }
}

/// Decode `n` unsorted tail values from `buf` at `*byte`, appending to `out`.
pub(super) fn decode_tail_unsorted(buf: &[u8], byte: &mut usize, n: usize, out: &mut Vec<u32>) {
    for _ in 0..n {
        out.push(vbyte::take(buf, byte));
    }
}

/// Lazy VByte tail decoder for a sorted list, shared by the `bp128` and
/// `bp128-skip` cursors. The first gap's delta base is the last full block's
/// final value when any full block precedes the tail, else the first value is
/// stored raw. `seed` supplies that base; call it before the first `next`
/// when `full`.
pub(super) struct SortedTail {
    full: bool,
    r: usize,
    last: u32,
}

impl SortedTail {
    pub(super) fn new(full: bool) -> Self {
        Self { full, r: 0, last: 0 }
    }

    pub(super) fn seed(&mut self, base: u32) {
        self.last = base;
    }

    /// Read the next tail value from `buf` at `*byte`, advancing it.
    pub(super) fn next(&mut self, buf: &[u8], byte: &mut usize) -> u32 {
        let gap = vbyte::take(buf, byte);
        self.r += 1;
        self.last = if self.full || self.r > 1 { self.last + gap + 1 } else { gap };
        self.last
    }

    pub(super) fn r(&self) -> usize {
        self.r
    }

    pub(super) fn last(&self) -> u32 {
        self.last
    }
}

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
                    initial = Some(encode_block_sorted(&bp, initial, block, out));
                }
                encode_tail_sorted(initial, list.chunks_exact(BLOCK_LEN).remainder(), out);
            }
            Kind::Unsorted => {
                for block in list.chunks_exact(BLOCK_LEN) {
                    encode_block_unsorted(&bp, block, out);
                }
                encode_tail_unsorted(list.chunks_exact(BLOCK_LEN).remainder(), out);
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
                    let last = decode_block_sorted(&bp, initial, buf, &mut byte, &mut block);
                    out.extend_from_slice(&block);
                    initial = Some(last);
                }
                decode_tail_sorted(initial, buf, &mut byte, n % BLOCK_LEN, out);
            }
            Kind::Unsorted => {
                let mut byte = 0usize;
                for _ in 0..n / BLOCK_LEN {
                    decode_block_unsorted(&bp, buf, &mut byte, &mut block);
                    out.extend_from_slice(&block);
                }
                decode_tail_unsorted(buf, &mut byte, n % BLOCK_LEN, out);
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
            tail: SortedTail::new(n / BLOCK_LEN > 0),
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
    tail: SortedTail,
}

impl Bp128Cursor<'_> {
    fn load_block(&mut self, b: usize) {
        let initial = (b > 0).then(|| self.block[BLOCK_LEN - 1]);
        decode_block_sorted(&self.bp, initial, self.buf, &mut self.byte, &mut self.block);
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
            while self.tail.r() <= i - self.full {
                self.next_tail();
            }
            self.tail.last()
        }
    }

    /// Decode the next tail value. `tail` is seeded from the last decoded
    /// block only when a full block exists — the tail's first gap is then
    /// `v - prev - 1`, otherwise the first gap is the value itself.
    fn next_tail(&mut self) {
        if self.tail.r() == 0 && self.full > 0 {
            self.tail.seed(self.block[BLOCK_LEN - 1]);
        }
        self.tail.next(self.buf, &mut self.byte);
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
