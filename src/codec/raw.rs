//! Control codec: 32 bits per value, no compression. Every other codec's
//! ratio and speed is read against this.

use crate::stream::Kind;

use super::{Caps, Codec, Cursor};

pub struct Raw;

impl Codec for Raw {
    fn name(&self) -> &'static str {
        "raw-u32"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: false, streaming_encoder: true, seek: true, random_access: true }
    }

    fn encode(&self, _kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        out.reserve(list.len() * 4);
        for &v in list {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }

    fn decode(&self, _kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        out.extend(buf[..n * 4].as_chunks::<4>().0.iter().map(|&c| u32::from_le_bytes(c)));
    }

    fn cursor<'a>(&self, _universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(RawCursor { buf: &buf[..n * 4], pos: usize::MAX }))
    }

    fn get(&self, _kind: Kind, _universe: u32, _n: usize, buf: &[u8], i: usize) -> Option<u32> {
        let b = &buf[i * 4..i * 4 + 4];
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// `pos` is the index of the current element; `usize::MAX` is "before the
/// first", wrapping to 0 on the first advance.
struct RawCursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl RawCursor<'_> {
    fn at(&self, i: usize) -> u32 {
        let b = &self.buf[i * 4..i * 4 + 4];
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
    fn len(&self) -> usize {
        self.buf.len() / 4
    }
}

impl Cursor for RawCursor<'_> {
    /// Binary search from the current position — the natural skip for a
    /// fixed-width layout.
    fn next_geq(&mut self, target: u32) -> Option<u32> {
        let (mut lo, mut hi) = (if self.pos == usize::MAX { 0 } else { self.pos }, self.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.at(mid) < target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        self.pos = lo;
        (lo < self.len()).then(|| self.at(lo))
    }

    fn next(&mut self) -> Option<u32> {
        self.pos = self.pos.wrapping_add(1);
        (self.pos < self.len()).then(|| self.at(self.pos))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::Raw);
    }
}
