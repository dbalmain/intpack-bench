//! intpack's `elias_fano` codec: plain Elias-Fano with a sampled select
//! index, so a fresh seek jumps straight to the target region rather than
//! walking the highs. `aux_bytes` reports the select samples — 4 bytes per
//! 256 elements, against sucds's ~1.3 bits/int of select1/select0 indices.

use crate::stream::Kind;

use intpack::Cursor as IpCursor;
use intpack::elias_fano;

use super::{Caps, Codec, Cursor};

pub struct IntpackEf;

impl Codec for IntpackEf {
    fn name(&self) -> &'static str {
        "ip-ef"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: true, streaming_encoder: false, seek: true, random_access: true }
    }

    fn encode(&self, _kind: Kind, universe: u32, list: &[u32], out: &mut Vec<u8>) {
        elias_fano::encode(universe, list, out);
    }

    fn decode(&self, _kind: Kind, universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        elias_fano::decode(universe, n, buf, out);
    }

    fn aux_bytes(&self, n: usize, _buf: &[u8]) -> Option<usize> {
        Some(elias_fano::aux_len(0, n))
    }

    fn cursor<'a>(&self, universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(EfCursor { cur: elias_fano::Cursor::new(universe, n, buf) }))
    }

    fn get(&self, _kind: Kind, universe: u32, n: usize, buf: &[u8], i: usize) -> Option<u32> {
        (i < n).then(|| elias_fano::get(universe, n, buf, i))
    }
}

struct EfCursor<'a> {
    cur: elias_fano::Cursor<'a>,
}

impl Cursor for EfCursor<'_> {
    fn next_geq(&mut self, target: u32) -> Option<u32> {
        IpCursor::next_geq(&mut self.cur, target)
    }

    fn next(&mut self) -> Option<u32> {
        IpCursor::next(&mut self.cur)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::IntpackEf);
    }
}
