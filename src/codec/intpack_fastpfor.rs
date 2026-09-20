//! intpack's FastPFor (128-int blocks).

use intpack::fastpfor;

use crate::stream::Kind;

use super::{Caps, Codec};

pub struct IntpackFastPFor;

impl Codec for IntpackFastPFor {
    fn name(&self) -> &'static str {
        "ip-fastpfor"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: false, streaming_encoder: true, seek: false, random_access: false }
    }

    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        match kind {
            Kind::Sorted => fastpfor::encode_sorted(list, out),
            Kind::Unsorted => fastpfor::encode(list, out),
        }
    }

    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        match kind {
            Kind::Sorted => fastpfor::decode_sorted(n, buf, out),
            Kind::Unsorted => fastpfor::decode(n, buf, out),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::IntpackFastPFor);
    }
}
