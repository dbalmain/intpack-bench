//! `roaring` crate's `RoaringBitmap`, in the portable on-disk format.
//! `open` deserialises once into an owning `Prepared`; the cursor is a bitmap
//! iterator using the native `advance_to` (first element `>= n`), and `get` is
//! the native `select`. The portable format costs a fixed header (12 bytes for
//! one container) plus 4 bytes of offsets/descriptions per container — that
//! overhead is inherent to the format.

use crate::stream::Kind;

use roaring::{RoaringBitmap, bitmap::Iter};

use super::{Caps, Codec, Cursor, Prepared};

pub struct Roaring;

impl Codec for Roaring {
    fn name(&self) -> &'static str {
        "roaring"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: true, streaming_encoder: true, seek: true, random_access: true }
    }

    fn encode(&self, _kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        let bitmap = match RoaringBitmap::from_sorted_iter(list.iter().copied()) {
            Ok(bitmap) => bitmap,
            // A non-sorted iterator only arises from a harness invariant
            // violation; falling back to per-value insert keeps the encode
            // total. Writing to a `Vec<u8>` cannot fail.
            Err(_) => list.iter().copied().collect(),
        };
        let _ = bitmap.serialize_into(out);
    }

    fn decode(&self, _kind: Kind, _universe: u32, _n: usize, buf: &[u8], out: &mut Vec<u32>) {
        let Ok(bitmap) = RoaringBitmap::deserialize_from(buf) else {
            return;
        };
        out.extend(bitmap.iter());
    }

    fn cursor<'a>(&self, _universe: u32, _n: usize, _buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        // `open` overrides this with an owning deserialisation; a cursor on
        // the raw bytes would have to decode on every query.
        None
    }

    fn open<'a>(&self, _kind: Kind, _universe: u32, _n: usize, buf: &'a [u8]) -> Option<Box<dyn Prepared + 'a>> {
        let Ok(bitmap) = RoaringBitmap::deserialize_from(buf) else {
            return None;
        };
        Some(Box::new(RoaringPrepared { bitmap }))
    }
}

struct RoaringPrepared {
    bitmap: RoaringBitmap,
}

impl Prepared for RoaringPrepared {
    fn cursor(&self) -> Option<Box<dyn Cursor + '_>> {
        Some(Box::new(RoaringCursor { iter: self.bitmap.iter(), cur: None }))
    }

    fn get(&self, i: usize) -> Option<u32> {
        self.bitmap.select(i as u32)
    }
}

struct RoaringCursor<'a> {
    iter: Iter<'a>,
    cur: Option<u32>,
}

impl Cursor for RoaringCursor<'_> {
    fn next_geq(&mut self, target: u32) -> Option<u32> {
        if let Some(c) = self.cur
            && c >= target
        {
            return Some(c);
        }
        self.iter.advance_to(target);
        self.cur = self.iter.next();
        self.cur
    }

    fn next(&mut self) -> Option<u32> {
        self.cur = self.iter.next();
        self.cur
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::Roaring);
    }
}
