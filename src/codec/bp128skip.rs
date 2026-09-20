//! `bp128` bitpacking with a skip table: one `(last_value, byte_offset)` pair
//! per full 128-block lets a cursor binary-search the table and decode only
//! the block it lands in, and `get` decode a single block by ordinal. The
//! packed section is byte-identical to `bp128`; the table is the only extra
//! bytes (`aux_bytes` reports it). This is the measurement of what a skip
//! structure costs in bits and buys in seek time.
//!
//! Layout, little-endian:
//!
//! 1. Skip table, one entry per full 128-block: `last_value: u32` (the
//!    block's final element, the delta base the next block needs) then
//!    `byte_offset: u32` (offset of the block's `num_bits` byte from the
//!    start of the packed section). Unsorted lists store `last_value = 0`;
//!    only `byte_offset` is used (for `get`).
//! 2. Packed section: exactly the `bp128` payload.
//!
//! The table length is `n / 128`, known from `n`, so it needs no header. A
//! list shorter than one block has an empty table and is byte-identical to
//! `bp128`.

use crate::stream::Kind;

use bitpacking::{BitPacker, BitPacker4x};

use super::bp128::{self, BLOCK_LEN, SortedTail};
use super::vbyte;
use super::{Caps, Codec, Cursor};

#[cfg(test)]
use std::cell::Cell;

/// `last_value` of table entry `b`.
fn table_last(buf: &[u8], b: usize) -> u32 {
    let e = &buf[b * 8..b * 8 + 4];
    u32::from_le_bytes([e[0], e[1], e[2], e[3]])
}

/// `byte_offset` of table entry `b`, relative to the packed section start.
fn table_offset(buf: &[u8], b: usize) -> usize {
    let e = &buf[b * 8 + 4..b * 8 + 8];
    u32::from_le_bytes([e[0], e[1], e[2], e[3]]) as usize
}

/// Byte offset where the VByte tail begins: just past the last full block's
/// packed bytes. Zero when there are no full blocks (the buffer is all tail).
fn tail_start(buf: &[u8], n: usize) -> usize {
    let n_blocks = n / BLOCK_LEN;
    let packed = n_blocks * 8;
    if n_blocks == 0 {
        return 0;
    }
    let off = table_offset(buf, n_blocks - 1);
    let nb = buf[packed + off];
    packed + off + 1 + BitPacker4x::compressed_block_size(nb)
}

pub struct Bp128Skip;

impl Codec for Bp128Skip {
    fn name(&self) -> &'static str {
        "bp128-skip"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: false, streaming_encoder: true, seek: true, random_access: true }
    }

    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        let bp = BitPacker4x::new();
        let n_blocks = list.len() / BLOCK_LEN;
        let table = out.len();
        out.resize(table + n_blocks * 8, 0);
        let packed = out.len();
        match kind {
            Kind::Sorted => {
                let mut initial: Option<u32> = None;
                for (b, block) in list.chunks_exact(BLOCK_LEN).enumerate() {
                    let byte_offset = (out.len() - packed) as u32;
                    let last = bp128::encode_block_sorted(&bp, initial, block, out);
                    let e = table + b * 8;
                    out[e..e + 4].copy_from_slice(&last.to_le_bytes());
                    out[e + 4..e + 8].copy_from_slice(&byte_offset.to_le_bytes());
                    initial = Some(last);
                }
                bp128::encode_tail_sorted(initial, list.chunks_exact(BLOCK_LEN).remainder(), out);
            }
            Kind::Unsorted => {
                for (b, block) in list.chunks_exact(BLOCK_LEN).enumerate() {
                    let byte_offset = (out.len() - packed) as u32;
                    bp128::encode_block_unsorted(&bp, block, out);
                    let e = table + b * 8;
                    out[e..e + 4].copy_from_slice(&0u32.to_le_bytes());
                    out[e + 4..e + 8].copy_from_slice(&byte_offset.to_le_bytes());
                }
                bp128::encode_tail_unsorted(list.chunks_exact(BLOCK_LEN).remainder(), out);
            }
        }
    }

    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        let bp = BitPacker4x::new();
        let n_blocks = n / BLOCK_LEN;
        let mut byte = n_blocks * 8;
        let mut block = [0u32; BLOCK_LEN];
        match kind {
            Kind::Sorted => {
                let mut initial: Option<u32> = None;
                for _ in 0..n_blocks {
                    let last = bp128::decode_block_sorted(&bp, initial, buf, &mut byte, &mut block);
                    out.extend_from_slice(&block);
                    initial = Some(last);
                }
                bp128::decode_tail_sorted(initial, buf, &mut byte, n % BLOCK_LEN, out);
            }
            Kind::Unsorted => {
                for _ in 0..n_blocks {
                    bp128::decode_block_unsorted(&bp, buf, &mut byte, &mut block);
                    out.extend_from_slice(&block);
                }
                bp128::decode_tail_unsorted(buf, &mut byte, n % BLOCK_LEN, out);
            }
        }
    }

    fn aux_bytes(&self, n: usize, _buf: &[u8]) -> Option<usize> {
        Some(n / BLOCK_LEN * 8)
    }

    fn cursor<'a>(&self, _universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(Bp128SkipCursor::new(n, buf)))
    }

    fn get(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], i: usize) -> Option<u32> {
        if i >= n {
            return None;
        }
        let bp = BitPacker4x::new();
        let n_blocks = n / BLOCK_LEN;
        let packed = n_blocks * 8;
        let full = n_blocks * BLOCK_LEN;
        let mut block = [0u32; BLOCK_LEN];
        if i < full {
            let b = i / BLOCK_LEN;
            let mut byte = packed + table_offset(buf, b);
            match kind {
                Kind::Sorted => {
                    let initial = (b > 0).then(|| table_last(buf, b - 1));
                    bp128::decode_block_sorted(&bp, initial, buf, &mut byte, &mut block);
                }
                Kind::Unsorted => bp128::decode_block_unsorted(&bp, buf, &mut byte, &mut block),
            }
            Some(block[i % BLOCK_LEN])
        } else {
            let mut byte = tail_start(buf, n);
            match kind {
                Kind::Sorted => {
                    let base = if n_blocks > 0 { table_last(buf, n_blocks - 1) } else { 0 };
                    let mut tail = SortedTail::new(n_blocks > 0);
                    if n_blocks > 0 {
                        tail.seed(base);
                    }
                    for _ in 0..=i - full {
                        tail.next(buf, &mut byte);
                    }
                    Some(tail.last())
                }
                Kind::Unsorted => {
                    let mut v = 0;
                    for _ in 0..=i - full {
                        v = vbyte::take(buf, &mut byte);
                    }
                    Some(v)
                }
            }
        }
    }
}

/// Skip cursor: binary-searches the table to decode only the block it lands
/// in, then scans within that block; the tail is entered from the table's
/// last entry without decoding any block.
struct Bp128SkipCursor<'a> {
    bp: BitPacker4x,
    buf: &'a [u8],
    n: usize,
    full: usize,
    n_blocks: usize,
    /// Byte position in `buf` for tail VByte reads.
    byte: usize,
    block: [u32; BLOCK_LEN],
    block_num: usize,
    idx: usize,
    cur: Option<u32>,
    tail: SortedTail,
    #[cfg(test)]
    decoded_blocks: Cell<usize>,
}

impl<'a> Bp128SkipCursor<'a> {
    fn new(n: usize, buf: &'a [u8]) -> Self {
        Self {
            bp: BitPacker4x::new(),
            buf,
            n,
            full: n / BLOCK_LEN * BLOCK_LEN,
            n_blocks: n / BLOCK_LEN,
            byte: 0,
            block: [0; BLOCK_LEN],
            block_num: usize::MAX,
            idx: usize::MAX,
            cur: None,
            tail: SortedTail::new(n / BLOCK_LEN > 0),
            #[cfg(test)]
            decoded_blocks: Cell::new(0),
        }
    }

    /// Decode block `b` via its table entry: `initial` is the previous
    /// block's `last_value`, so a jump never decodes the preceding block.
    fn load_block(&mut self, b: usize) {
        let initial = (b > 0).then(|| table_last(self.buf, b - 1));
        let mut byte = self.n_blocks * 8 + table_offset(self.buf, b);
        bp128::decode_block_sorted(&self.bp, initial, self.buf, &mut byte, &mut self.block);
        self.block_num = b;
        #[cfg(test)]
        self.decoded_blocks.set(self.decoded_blocks.get() + 1);
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

    /// Read the next tail value, entering the tail on first use: the delta
    /// base is the last full block's `last_value` from the table, so no block
    /// has to be decoded to reach the tail.
    fn next_tail(&mut self) -> u32 {
        if self.tail.r() == 0 {
            self.byte = tail_start(self.buf, self.n);
            if self.n_blocks > 0 {
                self.tail.seed(table_last(self.buf, self.n_blocks - 1));
            }
        }
        self.tail.next(self.buf, &mut self.byte)
    }

    /// Scan the VByte tail for the first value `>= target`, from the current
    /// tail position.
    fn scan_tail(&mut self, target: u32) -> Option<u32> {
        while self.tail.r() < self.n - self.full {
            let v = self.next_tail();
            if v >= target {
                self.idx = self.full + self.tail.r() - 1;
                self.cur = Some(v);
                return Some(v);
            }
        }
        self.cur = None;
        None
    }
}

impl Cursor for Bp128SkipCursor<'_> {
    fn next_geq(&mut self, target: u32) -> Option<u32> {
        if let Some(c) = self.cur
            && c >= target
        {
            return Some(c);
        }
        // First block at or after the current one whose last_value >= target
        // (targets are monotone, so never search behind the current block).
        let cur_block = if self.idx == usize::MAX { 0 } else { self.idx / BLOCK_LEN };
        let (mut lo, mut hi) = (cur_block.min(self.n_blocks), self.n_blocks);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if table_last(self.buf, mid) < target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo < self.n_blocks {
            if self.block_num != lo {
                self.load_block(lo);
            }
            let from = if lo == cur_block && self.idx != usize::MAX { self.idx % BLOCK_LEN } else { 0 };
            for j in from..BLOCK_LEN {
                if self.block[j] >= target {
                    self.idx = lo * BLOCK_LEN + j;
                    self.cur = Some(self.block[j]);
                    return Some(self.block[j]);
                }
            }
            unreachable!("block {lo} has last_value >= target, so the scan must hit");
        }
        self.scan_tail(target)
    }

    fn next(&mut self) -> Option<u32> {
        self.idx = self.idx.wrapping_add(1);
        self.cur = (self.idx < self.n).then(|| self.value_at(self.idx));
        self.cur
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conformance() {
        super::super::conformance(&Bp128Skip);
    }

    /// Encode a sorted list and return (buffer, n).
    fn encode_sorted(list: &[u32]) -> Vec<u8> {
        let mut buf = Vec::new();
        Bp128Skip.encode(Kind::Sorted, 1 << 30, list, &mut buf);
        buf
    }

    #[test]
    fn jump_decodes_only_target_block() {
        // 100 full blocks, dense enough that each block has a distinct max.
        let list: Vec<u32> = (0..12800).map(|i| i * 100).collect();
        let buf = encode_sorted(&list);
        let mut cur = Bp128SkipCursor::new(list.len(), &buf);
        // Block 90 holds indices 11520..11648, values 1_152_000..1_164_700.
        let target = 1_152_005;
        assert_eq!(cur.next_geq(target), Some(1_152_100));
        assert_eq!(cur.decoded_blocks.get(), 1);
        // Re-seek within the same block must not decode again.
        assert_eq!(cur.next_geq(1_160_000), Some(1_160_000));
        assert_eq!(cur.decoded_blocks.get(), 1);
    }

    #[test]
    fn tail_from_fresh_cursor_needs_no_block_decode() {
        // 2 full blocks + a 60-value tail.
        let list: Vec<u32> = (0..256 + 60).map(|i| i * 7 + 1).collect();
        let buf = encode_sorted(&list);
        let mut cur = Bp128SkipCursor::new(list.len(), &buf);
        let target = list[256]; // first tail element
        assert_eq!(cur.next_geq(target), Some(target));
        assert_eq!(cur.decoded_blocks.get(), 0);
        // And past the end of the list: exhausted.
        assert_eq!(cur.next_geq(1 << 30), None);
    }

    #[test]
    fn tail_after_block_decode() {
        let list: Vec<u32> = (0..256 + 60).map(|i| i * 7 + 1).collect();
        let buf = encode_sorted(&list);
        let mut cur = Bp128SkipCursor::new(list.len(), &buf);
        // Seek into block 0 first, then jump into the tail.
        assert_eq!(cur.next_geq(list[100]), Some(list[100]));
        let target = list[270];
        assert_eq!(cur.next_geq(target), Some(target));
        assert_eq!(cur.next_geq(list[315]), Some(list[315]));
        assert_eq!(cur.next_geq(1 << 30), None);
    }

    #[test]
    fn get_positions() {
        // 2 full blocks + a 44-value tail (n = 300).
        let list: Vec<u32> = (0..300).map(|i| i * 7 + 3).collect();
        let buf = encode_sorted(&list);
        let get = |i| Bp128Skip.get(Kind::Sorted, 1 << 30, list.len(), &buf, i);
        assert_eq!(get(0), Some(list[0]));
        assert_eq!(get(127), Some(list[127]));
        assert_eq!(get(128), Some(list[128]));
        assert_eq!(get(255), Some(list[255])); // last full-block element
        assert_eq!(get(256), Some(list[256])); // first tail element
        assert_eq!(get(299), Some(list[299])); // last element
        assert_eq!(get(300), None);
    }

    #[test]
    fn get_unsorted() {
        let list: Vec<u32> = (0..300u32).map(|i| i.wrapping_mul(2_654_435_761) % 1000).collect();
        let mut buf = Vec::new();
        Bp128Skip.encode(Kind::Unsorted, 1000, &list, &mut buf);
        for (i, &v) in list.iter().enumerate() {
            assert_eq!(Bp128Skip.get(Kind::Unsorted, 1000, list.len(), &buf, i), Some(v));
        }
        assert_eq!(Bp128Skip.get(Kind::Unsorted, 1000, list.len(), &buf, list.len()), None);
    }

    #[test]
    fn short_list_is_byte_identical_to_bp128() {
        // A list under one block has an empty table.
        let list: Vec<u32> = (0..77).map(|i| i * 3 + 1).collect();
        let mut skip = Vec::new();
        Bp128Skip.encode(Kind::Sorted, 1 << 30, &list, &mut skip);
        let mut plain = Vec::new();
        super::super::bp128::Bp128.encode(Kind::Sorted, 1 << 30, &list, &mut plain);
        assert_eq!(skip, plain);
    }
}
