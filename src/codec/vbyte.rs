//! Reference VByte: 7 payload bits + 1 continuation bit per byte, delta-coded
//! for sorted input (gap minus one, since gaps are ≥ 1). No skip structure —
//! the cursor is a linear scan, which is the honest baseline that skip
//! structures must beat by more than they cost in bytes.

use crate::stream::Kind;

use super::{Caps, Codec, Cursor};

pub struct VByte;

pub fn put(mut v: u32, out: &mut Vec<u8>) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// Read one varint at `*pos`, advancing it.
pub fn take(buf: &[u8], pos: &mut usize) -> u32 {
    let mut v = 0u32;
    let mut shift = 0;
    loop {
        let b = buf[*pos];
        *pos += 1;
        v |= u32::from(b & 0x7f) << shift;
        if b < 0x80 {
            return v;
        }
        shift += 7;
    }
}

impl Codec for VByte {
    fn name(&self) -> &'static str {
        "vbyte"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: false, streaming_encoder: true, seek: true, random_access: false }
    }

    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        match kind {
            Kind::Sorted => {
                let mut prev = 0u32;
                for (i, &v) in list.iter().enumerate() {
                    // First gap is from -1 so that a list starting at 0 costs a zero.
                    let gap = if i == 0 { v } else { v - prev - 1 };
                    put(gap, out);
                    prev = v;
                }
            }
            Kind::Unsorted => {
                for &v in list {
                    put(v, out);
                }
            }
        }
    }

    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        let mut pos = 0;
        match kind {
            Kind::Sorted => {
                let mut prev = 0u32;
                for i in 0..n {
                    let gap = take(buf, &mut pos);
                    let v = if i == 0 { gap } else { prev + gap + 1 };
                    out.push(v);
                    prev = v;
                }
            }
            Kind::Unsorted => {
                for _ in 0..n {
                    out.push(take(buf, &mut pos));
                }
            }
        }
    }

    fn cursor<'a>(&self, _universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(VByteCursor { buf, pos: 0, remaining: n, current: None }))
    }
}

struct VByteCursor<'a> {
    buf: &'a [u8],
    pos: usize,
    remaining: usize,
    /// `None` before the first element.
    current: Option<u32>,
}

impl Cursor for VByteCursor<'_> {
    fn next_geq(&mut self, target: u32) -> Option<u32> {
        if let Some(c) = self.current
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
        if self.remaining == 0 {
            self.current = None;
            return None;
        }
        self.remaining -= 1;
        let gap = take(self.buf, &mut self.pos);
        let v = match self.current {
            Some(prev) => prev + gap + 1,
            None => gap,
        };
        self.current = Some(v);
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::VByte);
    }
}
