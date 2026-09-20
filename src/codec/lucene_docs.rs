//! Lucene 10.3.1 docs-only postings (`IndexOptions.DOCS`, no freqs/positions):
//! the `.doc` stream `Lucene103PostingsWriter` writes and
//! `Lucene103PostingsReader.BlockPostingsEnum` reads, including both skip
//! levels. The cursor is Lucene's `advance` / `nextDoc` over that stream.
//!
//! One intended divergence: Lucene inlines `docFreq == 1` in the terms
//! dictionary and writes nothing to `.doc`; we write the one-doc group-vint
//! block instead.

use crate::stream::Kind;

use super::lucene::BLOCK_SIZE;
use super::lucene::for_delta;
use super::lucene::for_util;
use super::lucene::io::{self, Reader, write_group_vints, write_vint, write_vint15, write_vlong, write_vlong15};
use super::{Caps, Codec, Cursor};

/// 32 blocks. Java `LEVEL1_MASK` is `LEVEL1_NUM_DOCS - 1`.
const LEVEL1_NUM_BLOCKS: usize = 32;
const LEVEL1_NUM_DOCS: usize = LEVEL1_NUM_BLOCKS * BLOCK_SIZE;

/// Lucene uses `Integer.MAX_VALUE`; an `i64` sentinel avoids colliding with a
/// `u32` doc id.
const NO_MORE_DOCS: i64 = i64::MAX;

pub struct LuceneDocs;

impl Codec for LuceneDocs {
    fn name(&self) -> &'static str {
        "lucene-docs"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: true, streaming_encoder: true, seek: true, random_access: false }
    }

    fn encode(&self, _kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        encode_docs(list, out);
    }

    fn decode(&self, _kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        decode_docs(n, buf, out);
    }

    fn aux_bytes(&self, _universe: u32, n: usize, buf: &[u8]) -> Option<usize> {
        Some(aux_bytes(n, buf))
    }

    fn cursor<'a>(&self, _universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(DocsCursor::new(n, buf)))
    }
}

fn encode_docs(list: &[u32], out: &mut Vec<u8>) {
    let mut level0_last: i64 = -1;
    let mut level1_last: i64 = -1;
    let mut prev: i64 = -1;
    let mut doc_count = 0usize;
    let mut level1 = Vec::new();
    let mut payload = Vec::new();
    let mut scratch = Vec::new();

    let (blocks, tail) = list.as_chunks::<BLOCK_SIZE>();
    for block in blocks {
        let mut deltas = [0u32; BLOCK_SIZE];
        for (i, &doc) in block.iter().enumerate() {
            deltas[i] = (i64::from(doc) - prev) as u32;
            prev = i64::from(doc);
        }
        let last_doc = i64::from(block[BLOCK_SIZE - 1]);
        let doc_range = last_doc - level0_last;

        payload.clear();
        encode_block_payload(&deltas, doc_range, &mut payload);

        scratch.clear();
        write_vint15(&mut scratch, doc_range as u32);
        write_vlong15(&mut scratch, payload.len() as u64);
        write_vlong(&mut level1, scratch.len() as u64);
        level1.extend_from_slice(&scratch);
        level1.extend_from_slice(&payload);

        level0_last = last_doc;
        doc_count += BLOCK_SIZE;
        if doc_count.is_multiple_of(LEVEL1_NUM_DOCS) {
            write_vint(out, (last_doc - level1_last) as u32);
            write_vlong(out, level1.len() as u64);
            out.append(&mut level1);
            level1_last = last_doc;
        }
    }

    if !tail.is_empty() {
        let mut deltas = Vec::with_capacity(tail.len());
        for &doc in tail {
            deltas.push((i64::from(doc) - prev) as u32);
            prev = i64::from(doc);
        }
        write_group_vints(&mut level1, &deltas);
    }
    out.append(&mut level1);
}

fn encode_block_payload(deltas: &[u32; BLOCK_SIZE], doc_range: i64, out: &mut Vec<u8>) {
    let bpv = for_delta::bits_required(deltas);
    let num_bits_next = i64::from(32.min(bpv + 1)) * BLOCK_SIZE as i64;
    if doc_range == BLOCK_SIZE as i64 {
        out.push(0);
    } else if num_bits_next <= doc_range {
        out.push(bpv as u8);
        for_delta::encode_deltas(bpv, deltas, out);
    } else {
        let num_longs = (doc_range as usize).div_ceil(64);
        debug_assert!(num_longs <= 64);
        out.push((-(num_longs as i8)) as u8);
        let mut words = [0u64; 64];
        let mut s: i64 = -1;
        for &d in deltas {
            s += i64::from(d);
            let idx = s as usize;
            words[idx / 64] |= 1u64 << (idx % 64);
        }
        for &w in words.iter().take(num_longs) {
            io::write_u64_le(out, w);
        }
    }
}

fn decode_docs(n: usize, buf: &[u8], out: &mut Vec<u32>) {
    let mut r = Reader::new(buf);
    let mut prev: i64 = -1;
    let n_full = n / BLOCK_SIZE;
    for i in 0..n_full {
        if i.is_multiple_of(LEVEL1_NUM_BLOCKS) && n - i * BLOCK_SIZE >= LEVEL1_NUM_DOCS {
            let _ = r.read_vint();
            let _ = r.read_vlong();
        }
        let _ = r.read_vlong();
        let _ = r.read_vint15();
        let _ = r.read_vlong15();
        prev = decode_full_block(&mut r, prev, out);
    }
    let tail = n % BLOCK_SIZE;
    if tail > 0 {
        let mut deltas = vec![0u32; tail];
        r.read_group_vints(&mut deltas);
        for d in deltas {
            prev += i64::from(d);
            out.push(prev as u32);
        }
    }
}

fn aux_bytes(n: usize, buf: &[u8]) -> usize {
    let mut r = Reader::new(buf);
    let mut aux = 0usize;
    let n_full = n / BLOCK_SIZE;
    for i in 0..n_full {
        if i.is_multiple_of(LEVEL1_NUM_BLOCKS) && n - i * BLOCK_SIZE >= LEVEL1_NUM_DOCS {
            let start = r.pos;
            let _ = r.read_vint();
            let _ = r.read_vlong();
            aux += r.pos - start;
        }
        let start = r.pos;
        let _ = r.read_vlong();
        let _ = r.read_vint15();
        let block_len = r.read_vlong15();
        aux += r.pos - start;
        r.skip(block_len as usize);
    }
    aux
}

fn decode_full_block(r: &mut Reader<'_>, prev: i64, out: &mut Vec<u32>) -> i64 {
    match read_full_block(r, prev) {
        DecodedBlock::Packed(block) => {
            out.extend_from_slice(&block);
            i64::from(block[BLOCK_SIZE - 1])
        }
        DecodedBlock::Unary { base, words, num_longs } => {
            let mut last = prev;
            for (i, &word) in words.iter().take(num_longs).enumerate() {
                let mut w = word;
                let offset = (i as i64) * 64;
                while w != 0 {
                    let bit = w.trailing_zeros();
                    last = base + offset + i64::from(bit);
                    out.push(last as u32);
                    w &= w - 1;
                }
            }
            last
        }
    }
}

enum DecodedBlock {
    Packed([u32; BLOCK_SIZE]),
    Unary { base: i64, words: [u64; 64], num_longs: usize },
}

fn read_full_block(r: &mut Reader<'_>, prev: i64) -> DecodedBlock {
    let token = r.read_byte() as i8;
    if token > 0 {
        let bpv = u32::from(token as u8);
        let rest = r.buf.get(r.pos..).unwrap_or(&[]);
        let mut block = [0u32; BLOCK_SIZE];
        for_delta::decode_and_prefix_sum(bpv, rest, prev as u32, &mut block);
        r.skip(for_util::num_bytes(bpv));
        DecodedBlock::Packed(block)
    } else if token == 0 {
        let mut words = [0u64; 64];
        words[0] = u64::MAX;
        words[1] = u64::MAX;
        DecodedBlock::Unary { base: prev + 1, words, num_longs: 2 }
    } else {
        let num_longs = (-token) as usize;
        let mut words = [0u64; 64];
        for w in words.iter_mut().take(num_longs.min(64)) {
            *w = r.read_u64_le();
        }
        DecodedBlock::Unary { base: prev + 1, words, num_longs: num_longs.min(64) }
    }
}

#[derive(Clone, Copy)]
enum Encoding {
    Packed,
    Unary,
}

/// `BlockPostingsEnum` docs-only state machine.
struct DocsCursor<'a> {
    r: Reader<'a>,
    n: usize,
    doc: i64,
    prev_doc: i64,
    doc_count_left: usize,
    level0_last_doc: i64,
    level0_end: usize,
    level1_last_doc: i64,
    level1_end: usize,
    level1_doc_count_upto: usize,
    doc_buffer: [i64; BLOCK_SIZE],
    doc_buffer_size: usize,
    doc_buffer_upto: usize,
    encoding: Encoding,
    words: [u64; 64],
    num_longs: usize,
    doc_bit_set_base: i64,
    needs_refilling: bool,
    #[cfg(test)]
    decoded_blocks: usize,
}

impl<'a> DocsCursor<'a> {
    fn new(n: usize, buf: &'a [u8]) -> Self {
        let (level1_last_doc, level1_end) = if n < LEVEL1_NUM_DOCS { (NO_MORE_DOCS, 0) } else { (-1, 0) };
        Self {
            r: Reader::new(buf),
            n,
            doc: -1,
            prev_doc: -1,
            doc_count_left: n,
            level0_last_doc: -1,
            level0_end: 0,
            level1_last_doc,
            level1_end,
            level1_doc_count_upto: 0,
            doc_buffer: [0; BLOCK_SIZE],
            doc_buffer_size: BLOCK_SIZE,
            doc_buffer_upto: BLOCK_SIZE,
            encoding: Encoding::Packed,
            words: [0; 64],
            num_longs: 0,
            doc_bit_set_base: 0,
            needs_refilling: false,
            #[cfg(test)]
            decoded_blocks: 0,
        }
    }

    fn yield_doc(&self) -> Option<u32> {
        if self.doc < 0 || self.doc == NO_MORE_DOCS { None } else { Some(self.doc as u32) }
    }

    fn refill_docs(&mut self) {
        if self.doc_count_left >= BLOCK_SIZE {
            self.refill_full_block();
        } else {
            self.refill_remainder();
        }
    }

    fn refill_full_block(&mut self) {
        match read_full_block(&mut self.r, self.prev_doc) {
            DecodedBlock::Packed(block) => {
                for (slot, &v) in self.doc_buffer.iter_mut().zip(block.iter()) {
                    *slot = i64::from(v);
                }
                self.prev_doc = i64::from(block[BLOCK_SIZE - 1]);
                self.encoding = Encoding::Packed;
            }
            DecodedBlock::Unary { base, words, num_longs } => {
                self.doc_bit_set_base = base;
                self.words = words;
                self.num_longs = num_longs;
                self.prev_doc = self.level0_last_doc;
                self.encoding = Encoding::Unary;
            }
        }
        self.doc_count_left -= BLOCK_SIZE;
        self.doc_buffer_size = BLOCK_SIZE;
        self.doc_buffer_upto = 0;
        #[cfg(test)]
        {
            self.decoded_blocks += 1;
        }
    }

    fn refill_remainder(&mut self) {
        let count = self.doc_count_left;
        if count > 0 {
            let mut tmp = [0u32; BLOCK_SIZE];
            self.r.read_group_vints(&mut tmp[..count]);
            let mut sum = self.prev_doc;
            for (i, &d) in tmp.iter().take(count).enumerate() {
                sum += i64::from(d);
                self.doc_buffer[i] = sum;
            }
            self.prev_doc = sum;
        }
        self.doc_buffer_size = count;
        self.doc_count_left = 0;
        self.doc_buffer_upto = 0;
        self.encoding = Encoding::Packed;
    }

    /// `skipLevel1To`.
    fn skip_level1_to(&mut self, target: i64) {
        loop {
            self.prev_doc = self.level1_last_doc;
            self.level0_last_doc = self.level1_last_doc;
            self.r.pos = self.level1_end.min(self.r.buf.len());
            self.doc_count_left = self.n.saturating_sub(self.level1_doc_count_upto);
            self.level1_doc_count_upto += LEVEL1_NUM_DOCS;
            if self.doc_count_left < LEVEL1_NUM_DOCS {
                self.level1_last_doc = NO_MORE_DOCS;
                break;
            }
            self.level1_last_doc += i64::from(self.r.read_vint());
            let delta = self.r.read_vlong();
            self.level1_end = self.r.pos.saturating_add(delta as usize);
            if self.level1_last_doc >= target {
                break;
            }
        }
    }

    /// `skipLevel0To` (docs-only: no impacts / positions).
    fn skip_level0_to(&mut self, target: i64) {
        loop {
            self.prev_doc = self.level0_last_doc;
            if self.doc_count_left >= BLOCK_SIZE {
                let _ = self.r.read_vlong();
                self.level0_last_doc += i64::from(self.r.read_vint15());
                let found = target <= self.level0_last_doc;
                let block_len = self.r.read_vlong15();
                self.level0_end = self.r.pos.saturating_add(block_len as usize);
                if found {
                    break;
                }
                self.r.pos = self.level0_end.min(self.r.buf.len());
                self.doc_count_left -= BLOCK_SIZE;
            } else {
                self.level0_last_doc = NO_MORE_DOCS;
                break;
            }
        }
    }

    fn do_advance_shallow(&mut self, target: i64) {
        if target > self.level1_last_doc {
            self.skip_level1_to(target);
        } else if self.needs_refilling {
            self.r.pos = self.level0_end.min(self.r.buf.len());
            self.doc_count_left -= BLOCK_SIZE;
        }
        self.skip_level0_to(target);
    }

    /// `doMoveToNextLevel0Block` remainder / non-fast path.
    fn do_move_to_next_level0_block(&mut self) {
        if self.doc_count_left >= BLOCK_SIZE {
            let _ = self.r.read_vlong();
            self.level0_last_doc += i64::from(self.r.read_vint15());
            let block_len = self.r.read_vlong15();
            self.level0_end = self.r.pos.saturating_add(block_len as usize);
            self.refill_full_block();
        } else {
            self.level0_last_doc = NO_MORE_DOCS;
            self.refill_remainder();
        }
    }

    /// `moveToNextLevel0Block`, taking the `needsDocsAndFreqsOnly` fast path.
    fn move_to_next_level0_block(&mut self) {
        if self.doc == self.level1_last_doc {
            self.skip_level1_to(self.doc + 1);
        }
        self.prev_doc = self.level0_last_doc;
        if self.doc_count_left >= BLOCK_SIZE {
            let num_skip = self.r.read_vlong();
            let level0_end = self.r.pos.saturating_add(num_skip as usize);
            self.level0_last_doc += i64::from(self.r.read_vint15());
            self.r.pos = level0_end.min(self.r.buf.len());
            self.refill_full_block();
        } else {
            self.do_move_to_next_level0_block();
        }
    }

    /// Lucene `nextDoc`.
    fn next_doc(&mut self) -> Option<u32> {
        if self.doc == self.level0_last_doc || self.needs_refilling {
            if self.needs_refilling {
                self.refill_docs();
                self.needs_refilling = false;
            } else {
                self.move_to_next_level0_block();
            }
        }
        match self.encoding {
            Encoding::Packed => {
                self.doc = if self.doc_buffer_upto >= self.doc_buffer_size {
                    NO_MORE_DOCS
                } else {
                    self.doc_buffer[self.doc_buffer_upto]
                };
            }
            Encoding::Unary => {
                let from = self.doc - self.doc_bit_set_base + 1;
                self.doc = match next_set_bit(&self.words, self.num_longs, from) {
                    Some(bit) => self.doc_bit_set_base + bit,
                    None => NO_MORE_DOCS,
                };
            }
        }
        self.doc_buffer_upto += 1;
        self.yield_doc()
    }

    /// Lucene `advance`; caller has already handled `target <= current`.
    fn advance(&mut self, target: i64) -> Option<u32> {
        if target > self.level0_last_doc || self.needs_refilling {
            if target > self.level0_last_doc {
                self.do_advance_shallow(target);
            }
            self.refill_docs();
            self.needs_refilling = false;
        }
        match self.encoding {
            Encoding::Packed => {
                let next = find_next_geq(&self.doc_buffer, target, self.doc_buffer_upto, self.doc_buffer_size);
                self.doc = if next >= self.doc_buffer_size { NO_MORE_DOCS } else { self.doc_buffer[next] };
                self.doc_buffer_upto = next + 1;
            }
            Encoding::Unary => {
                let from = target - self.doc_bit_set_base;
                self.doc = match next_set_bit(&self.words, self.num_longs, from) {
                    Some(bit) => self.doc_bit_set_base + bit,
                    None => NO_MORE_DOCS,
                };
                self.doc_buffer_upto = 1;
            }
        }
        self.yield_doc()
    }
}

impl Cursor for DocsCursor<'_> {
    fn next_geq(&mut self, target: u32) -> Option<u32> {
        let target = i64::from(target);
        if self.doc >= 0 && self.doc != NO_MORE_DOCS && self.doc >= target {
            return Some(self.doc as u32);
        }
        if self.doc == NO_MORE_DOCS {
            return None;
        }
        self.advance(target)
    }

    fn next(&mut self) -> Option<u32> {
        if self.doc == NO_MORE_DOCS {
            return None;
        }
        self.next_doc()
    }
}

fn find_next_geq(buf: &[i64; BLOCK_SIZE], target: i64, from: usize, to: usize) -> usize {
    let mut i = from;
    while i < to {
        if buf[i] >= target {
            return i;
        }
        i += 1;
    }
    to
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::Kind;

    #[test]
    fn conformance() {
        super::super::conformance(&LuceneDocs);
    }

    #[test]
    fn docs_fixtures_byte_exact() {
        let cases = super::super::lucene::docs_fixtures();
        assert_eq!(cases.len(), 13, "expected 13 docs fixture lines");
        for (i, (list, expected)) in cases.iter().enumerate() {
            let universe = list.last().copied().unwrap_or(0).saturating_add(1).max(1);
            let mut got = Vec::new();
            LuceneDocs.encode(Kind::Sorted, universe, list, &mut got);
            assert_eq!(&got, expected, "docs fixture {i} n={} encode", list.len());
            let mut back = Vec::new();
            LuceneDocs.decode(Kind::Sorted, universe, list.len(), &got, &mut back);
            assert_eq!(&back, list, "docs fixture {i} n={} decode", list.len());
            if let Err(e) = super::super::check(&LuceneDocs, Kind::Sorted, universe, list) {
                panic!("docs fixture {i} n={}: {e}", list.len());
            }
        }
    }

    fn fixture_n(n: usize) -> (Vec<u32>, Vec<u8>) {
        super::super::lucene::docs_fixtures()
            .into_iter()
            .find(|(list, _)| list.len() == n)
            .unwrap_or_else(|| panic!("no docs fixture with n={n}"))
    }

    #[test]
    fn seek_last_block_via_skip() {
        let (list, bytes) = fixture_n(10_000);
        let full = list.len() / BLOCK_SIZE * BLOCK_SIZE;
        let last_block_start = full - BLOCK_SIZE;
        let target = list[last_block_start];
        let mut cur = DocsCursor::new(list.len(), &bytes);
        assert_eq!(cur.next_geq(target), Some(target));
        assert_eq!(cur.decoded_blocks, 1, "skip data should land in the last full block without decoding the rest");
        assert_eq!(cur.next(), Some(list[last_block_start + 1]));
        assert_eq!(cur.next_geq(list[full - 1]), Some(list[full - 1]));
        assert_eq!(cur.next(), Some(list[full]));
        let last = list[list.len() - 1];
        assert_eq!(cur.next_geq(last), Some(last));
        assert_eq!(cur.next(), None);
    }

    #[test]
    fn seek_past_end() {
        let (list, bytes) = fixture_n(10_000);
        let mut cur = DocsCursor::new(list.len(), &bytes);
        let past = list[list.len() - 1] + 1;
        assert_eq!(cur.next_geq(past), None);
        assert_eq!(cur.next(), None);
        assert_eq!(cur.next_geq(past), None);
    }
}
