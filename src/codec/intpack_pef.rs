//! intpack's `pef` codec: fixed-128 partitioned Elias-Fano whose upper level
//! carries the select samples, so a fresh seek jumps to the target partition
//! instead of the reference `pef`'s binary search with a linear select each
//! probe. `aux_bytes` reports header + directory + upper select samples.

use crate::stream::Kind;

use intpack::Cursor as IpCursor;
use intpack::pef;

use super::{Caps, Codec, Cursor};

pub struct IntpackPef;

impl Codec for IntpackPef {
    fn name(&self) -> &'static str {
        "ip-pef"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: true, streaming_encoder: false, seek: true, random_access: true }
    }

    fn encode(&self, _kind: Kind, universe: u32, list: &[u32], out: &mut Vec<u8>) {
        pef::encode(universe, list, out);
    }

    fn decode(&self, _kind: Kind, universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        pef::decode(universe, n, buf, out);
    }

    fn aux_bytes(&self, n: usize, buf: &[u8]) -> Option<usize> {
        Some(pef::aux_bytes(n, buf))
    }

    fn cursor<'a>(&self, universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(PefCursor { cur: pef::Cursor::new(universe, n, buf) }))
    }

    fn get(&self, _kind: Kind, universe: u32, n: usize, buf: &[u8], i: usize) -> Option<u32> {
        (i < n).then(|| pef::get(universe, n, buf, i))
    }
}

struct PefCursor<'a> {
    cur: pef::Cursor<'a>,
}

impl Cursor for PefCursor<'_> {
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
        super::super::conformance(&super::IntpackPef);
    }
}
