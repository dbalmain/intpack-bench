//! `intpack`'s own `bp128` and `bp128skip`: the same container formats as
//! the `bitpacking`-crate rows (`bp128`, `bp128-skip`), so the comparison is
//! kernel against kernel. Each adapter is a thin call into the crate.

use crate::stream::Kind;

use intpack::{bp128, bp128skip};

use super::{Caps, Codec, Cursor};

/// `intpack::Cursor` has the same contract as the harness's.
struct Adapt<C>(C);

impl<C: intpack::Cursor> Cursor for Adapt<C> {
    fn next_geq(&mut self, target: u32) -> Option<u32> {
        self.0.next_geq(target)
    }

    fn next(&mut self) -> Option<u32> {
        self.0.next()
    }
}

pub struct IpBp128;

impl Codec for IpBp128 {
    fn name(&self) -> &'static str {
        "ip-bp128"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: false, streaming_encoder: true, seek: true, random_access: false }
    }

    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        match kind {
            Kind::Sorted => bp128::encode_sorted(list, out),
            Kind::Unsorted => bp128::encode(list, out),
        }
    }

    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        match kind {
            Kind::Sorted => bp128::decode_sorted(n, buf, out),
            Kind::Unsorted => bp128::decode(n, buf, out),
        }
    }

    fn cursor<'a>(&self, _universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(Adapt(bp128::SortedCursor::new(n, buf))))
    }
}

pub struct IpBp128Skip;

impl Codec for IpBp128Skip {
    fn name(&self) -> &'static str {
        "ip-bp128-skip"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: false, streaming_encoder: true, seek: true, random_access: true }
    }

    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        match kind {
            Kind::Sorted => bp128skip::encode_sorted(list, out),
            Kind::Unsorted => bp128skip::encode(list, out),
        }
    }

    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        match kind {
            Kind::Sorted => bp128skip::decode_sorted(n, buf, out),
            Kind::Unsorted => bp128skip::decode(n, buf, out),
        }
    }

    fn aux_bytes(&self, n: usize, _buf: &[u8]) -> Option<usize> {
        Some(bp128skip::aux_len(n))
    }

    fn cursor<'a>(&self, _universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(Adapt(bp128skip::SortedCursor::new(n, buf))))
    }

    fn get(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], i: usize) -> Option<u32> {
        (i < n).then(|| match kind {
            Kind::Sorted => bp128skip::get_sorted(n, buf, i),
            Kind::Unsorted => bp128skip::get(n, buf, i),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conformance() {
        super::super::conformance(&IpBp128);
        super::super::conformance(&IpBp128Skip);
    }

    /// The payloads match the `bitpacking`-crate rows byte for byte, so the
    /// bench compares kernels, not formats.
    #[test]
    fn byte_identical_to_crate_rows() {
        let list: Vec<u32> = (0..1000u32).map(|i| i * i).collect();
        for (kind, universe) in [(Kind::Sorted, 1 << 20), (Kind::Unsorted, 1 << 20)] {
            let (mut ours, mut theirs) = (Vec::new(), Vec::new());
            IpBp128.encode(kind, universe, &list, &mut ours);
            super::super::bp128::Bp128.encode(kind, universe, &list, &mut theirs);
            assert_eq!(ours, theirs, "bp128 {kind:?}");
            let (mut ours, mut theirs) = (Vec::new(), Vec::new());
            IpBp128Skip.encode(kind, universe, &list, &mut ours);
            super::super::bp128skip::Bp128Skip.encode(kind, universe, &list, &mut theirs);
            assert_eq!(ours, theirs, "bp128-skip {kind:?}");
        }
    }
}
