//! intpack's Stream VByte.

use intpack::streamvbyte;

use crate::stream::Kind;

use super::{Caps, Codec};

pub struct IntpackStreamVByte;

impl Codec for IntpackStreamVByte {
    fn name(&self) -> &'static str {
        "ip-streamvbyte"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: false, streaming_encoder: true, seek: false, random_access: false }
    }

    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        match kind {
            Kind::Sorted => streamvbyte::encode_sorted(list, out),
            Kind::Unsorted => streamvbyte::encode(list, out),
        }
    }

    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        match kind {
            Kind::Sorted => streamvbyte::decode_sorted(n, buf, out),
            Kind::Unsorted => streamvbyte::decode(n, buf, out),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::IntpackStreamVByte);
    }
}
