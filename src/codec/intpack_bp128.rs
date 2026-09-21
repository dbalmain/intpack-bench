//! `intpack`'s own `bp128` and `bp128skip`: the same 128-block, VByte-tail,
//! and optional skip-table container as the `bitpacking`-crate rows (`bp128`,
//! `bp128-skip`), extended with Lucene's at-most-seven patched exceptions per
//! block. Blocks without exceptions remain byte-identical. Each adapter is a
//! thin call into the crate.

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

    fn aux_bytes(&self, _universe: u32, n: usize, _buf: &[u8]) -> Option<usize> {
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

    #[test]
    fn never_longer_than_crate_rows_on_conformance_cases() {
        for kind in [Kind::Sorted, Kind::Unsorted] {
            for (universe, list) in super::super::conformance_cases(kind) {
                for (ours, theirs) in [
                    (&IpBp128 as &dyn Codec, &super::super::bp128::Bp128 as &dyn Codec),
                    (&IpBp128Skip as &dyn Codec, &super::super::bp128skip::Bp128Skip as &dyn Codec),
                ] {
                    let (mut ours_buf, mut theirs_buf) = (Vec::new(), Vec::new());
                    ours.encode(kind, universe, &list, &mut ours_buf);
                    theirs.encode(kind, universe, &list, &mut theirs_buf);
                    assert!(
                        ours_buf.len() <= theirs_buf.len(),
                        "{} longer than {} for {kind:?}, n={}",
                        ours.name(),
                        theirs.name(),
                        list.len()
                    );
                }
            }
        }
    }

    #[test]
    fn outlier_block_is_shorter_than_crate_rows() {
        let mut gaps = [511u32; 128];
        gaps[64] = 86_000;
        let mut prev = u32::MAX;
        let list: Vec<u32> = gaps
            .into_iter()
            .map(|gap| {
                prev = prev.wrapping_add(gap).wrapping_add(1);
                prev
            })
            .collect();
        for (ours, theirs) in [
            (&IpBp128 as &dyn Codec, &super::super::bp128::Bp128 as &dyn Codec),
            (&IpBp128Skip as &dyn Codec, &super::super::bp128skip::Bp128Skip as &dyn Codec),
        ] {
            let (mut ours_buf, mut theirs_buf) = (Vec::new(), Vec::new());
            ours.encode(Kind::Sorted, 1 << 20, &list, &mut ours_buf);
            theirs.encode(Kind::Sorted, 1 << 20, &list, &mut theirs_buf);
            assert!(ours_buf.len() < theirs_buf.len());
        }
    }
}
