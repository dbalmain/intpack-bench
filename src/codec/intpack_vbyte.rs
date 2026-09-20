//! intpack's LEB128 VByte, including a linear-scan cursor.

use intpack::Cursor as _;
use intpack::vbyte::{self, SortedCursor};

use crate::stream::Kind;

use super::{Caps, Codec, Cursor};

pub struct IntpackVByte;

impl Codec for IntpackVByte {
    fn name(&self) -> &'static str {
        "ip-vbyte"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: false, streaming_encoder: true, seek: true, random_access: false }
    }

    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        match kind {
            Kind::Sorted => vbyte::encode_sorted(list, out),
            Kind::Unsorted => vbyte::encode(list, out),
        }
    }

    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        match kind {
            Kind::Sorted => vbyte::decode_sorted(n, buf, out),
            Kind::Unsorted => vbyte::decode(n, buf, out),
        }
    }

    fn cursor<'a>(&self, _universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(Wrap(SortedCursor::new(n, buf))))
    }
}

struct Wrap<'a>(SortedCursor<'a>);

impl Cursor for Wrap<'_> {
    fn next_geq(&mut self, target: u32) -> Option<u32> {
        self.0.next_geq(target)
    }
    fn next(&mut self) -> Option<u32> {
        self.0.next()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::IntpackVByte);
    }
}
