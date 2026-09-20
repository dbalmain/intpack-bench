//! The codec contract and the registry of codecs under test.
//!
//! One trait rather than the usual enum-per-feature shape, deliberately: every
//! codec here is a self-contained adapter over a third-party crate (or a
//! reference implementation), and a new one is "add a file, add a registry
//! line". An enum would spread each codec across four match blocks for no
//! gain — nothing in the harness cares which codec it holds.
//!
//! Contract every adapter must honour:
//!
//! * `encode` gets one list. The list's length and the dataset universe are
//!   stored by the harness (as an index's term dictionary would), so codecs
//!   must not spend bytes on them, and `decode`/`cursor`/`get` receive them
//!   back. A codec that needs extra header state writes it into the buffer.
//! * Sorted lists are strictly increasing; delta coding is the codec's job.
//! * Unsorted lists are arbitrary `u32`; a codec that only handles sorted
//!   input reports so via [`Caps::sorted_only`] and is skipped on unsorted
//!   datasets.
//! * `decode` must produce exactly the original list. The harness checks.
//! * `cursor` and `get` are optional. Returning `None` means the codec has no
//!   native support and that cell is left blank in the report. Do **not**
//!   emulate them by decoding everything — that measures the wrong thing.
//! * `open` turns encoded bytes into whatever the codec needs to answer
//!   cursor/get queries — a pointer cast for zero-copy formats, a
//!   deserialisation for crates whose structures own their memory. The
//!   harness prepares each list once, times that as "open", and then times
//!   cursor and get operations on the prepared form. Override it when the
//!   default (which just holds the bytes and calls `cursor`/`get`) would
//!   deserialise on every query.

use crate::stream::Kind;

pub mod bp128;
pub mod bp128skip;
#[cfg(feature = "cpp")]
pub mod cpp;
pub mod eliasfano;
pub mod fastpfor;
pub mod intpack_bp128;
pub mod intpack_ef;
pub mod intpack_fastpfor;
pub mod intpack_pef;
pub mod intpack_streamvbyte;
pub mod intpack_vbyte;
pub mod pef;
pub mod raw;
pub mod roaring;
pub mod streamvbyte;
pub mod vbyte;

/// What a codec can do natively; drives which benchmarks run against it.
#[derive(Clone, Copy, Debug)]
pub struct Caps {
    /// Only accepts strictly increasing input.
    pub sorted_only: bool,
    /// The encoder can emit output before it has seen the whole list
    /// (VByte yes; partitioned Elias-Fano and interpolative coding no).
    /// Reported, not measured: it decides whether an index writer needs the
    /// whole posting list in memory.
    pub streaming_encoder: bool,
    /// [`Codec::cursor`] returns `Some`.
    pub seek: bool,
    /// [`Codec::get`] returns `Some`.
    pub random_access: bool,
}

/// Forward iteration with skip over one encoded sorted list.
pub trait Cursor {
    /// Position on the smallest value `>= target` at or after the current
    /// element and return it, or `None` once exhausted. The cursor stays
    /// **on** that element: if the current element already satisfies the
    /// target the call returns it again without moving. Targets must be
    /// non-decreasing. This is Lucene's `advance` and the leapfrog primitive
    /// intersections are built on.
    fn next_geq(&mut self, target: u32) -> Option<u32>;
    /// Move to the element after the current one and return it. A fresh
    /// cursor is positioned before the first element, so the first call
    /// returns it.
    fn next(&mut self) -> Option<u32>;
}

/// One list, opened for queries. See [`Codec::prepare`].
pub trait Prepared {
    fn cursor(&self) -> Option<Box<dyn Cursor + '_>>;
    fn get(&self, i: usize) -> Option<u32>;
}

/// Default [`Prepared`]: holds the bytes and defers to the codec's
/// `cursor`/`get`.
struct Lazy<'a> {
    codec: &'a dyn Codec,
    kind: Kind,
    universe: u32,
    n: usize,
    buf: &'a [u8],
}

impl Prepared for Lazy<'_> {
    fn cursor(&self) -> Option<Box<dyn Cursor + '_>> {
        self.codec.cursor(self.universe, self.n, self.buf)
    }
    fn get(&self, i: usize) -> Option<u32> {
        self.codec.get(self.kind, self.universe, self.n, self.buf, i)
    }
}

pub trait Codec: Sync {
    fn name(&self) -> &'static str;
    fn caps(&self) -> Caps;

    /// Encode one list, appending to `out`. `universe` is the exclusive upper
    /// bound on values for the whole dataset.
    fn encode(&self, kind: Kind, universe: u32, list: &[u32], out: &mut Vec<u8>);

    /// Decode a list of `n` values from `buf` (exactly the bytes `encode`
    /// produced for it), appending to `out`.
    fn decode(&self, kind: Kind, universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>);

    /// Bytes of `buf` spent on skip/index structures rather than payload, if
    /// the codec can tell. Reported as aux bits/int. `universe` is the same
    /// out-of-band bound passed to encode and decode.
    fn aux_bytes(&self, _universe: u32, _n: usize, _buf: &[u8]) -> Option<usize> {
        None
    }

    /// Native skip cursor over a sorted list, if supported.
    fn cursor<'a>(&self, _universe: u32, _n: usize, _buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        None
    }

    /// Native random access by ordinal, if supported.
    fn get(&self, _kind: Kind, _universe: u32, _n: usize, _buf: &[u8], _i: usize) -> Option<u32> {
        None
    }

    /// Open one encoded list for repeated cursor/get queries. `None` (the
    /// default) means the format is queried in place and [`prepare`] will
    /// hold the bytes and call `cursor`/`get` directly; owning structures
    /// override this to deserialise once.
    fn open<'a>(&self, _kind: Kind, _universe: u32, _n: usize, _buf: &'a [u8]) -> Option<Box<dyn Prepared + 'a>> {
        None
    }
}

/// Open one list for queries: the codec's own [`Codec::open`] if it has one,
/// else the in-place default.
pub fn prepare<'a>(codec: &'a dyn Codec, kind: Kind, universe: u32, n: usize, buf: &'a [u8]) -> Box<dyn Prepared + 'a> {
    codec.open(kind, universe, n, buf).unwrap_or_else(|| Box::new(Lazy { codec, kind, universe, n, buf }))
}

/// Every codec the harness knows, in report order. Reference codecs first.
pub fn all() -> Vec<Box<dyn Codec>> {
    vec![
        Box::new(raw::Raw),
        Box::new(vbyte::VByte),
        Box::new(intpack_vbyte::IntpackVByte),
        Box::new(bp128::Bp128),
        Box::new(bp128skip::Bp128Skip),
        Box::new(intpack_bp128::IpBp128),
        Box::new(intpack_bp128::IpBp128Skip),
        Box::new(fastpfor::FastPFor),
        Box::new(intpack_fastpfor::IntpackFastPFor),
        Box::new(streamvbyte::StreamVByte),
        Box::new(intpack_streamvbyte::IntpackStreamVByte),
        Box::new(roaring::Roaring),
        Box::new(eliasfano::EliasFanoCodec),
        Box::new(intpack_ef::IntpackEf),
        Box::new(pef::Pef),
        Box::new(intpack_pef::IntpackPef),
        #[cfg(feature = "cpp")]
        Box::new(cpp::CppSimdFastPFor128),
        #[cfg(feature = "cpp")]
        Box::new(cpp::CppSimdBinaryPacking),
        #[cfg(feature = "cpp")]
        Box::new(cpp::CppOptPFor),
        #[cfg(feature = "cpp")]
        Box::new(cpp::CppSimdPFor),
        #[cfg(feature = "cpp")]
        Box::new(cpp::CppBP32),
    ]
}

pub fn by_name(name: &str) -> Option<Box<dyn Codec>> {
    all().into_iter().find(|c| c.name() == name)
}

/// Round-trip and cursor/get conformance check, used by every adapter's tests
/// and by the harness before timing anything.
pub fn check(codec: &dyn Codec, kind: Kind, universe: u32, list: &[u32]) -> Result<(), String> {
    let mut buf = Vec::new();
    codec.encode(kind, universe, list, &mut buf);
    let mut back = Vec::new();
    codec.decode(kind, universe, list.len(), &buf, &mut back);
    if back != list {
        return Err(format!(
            "{}: roundtrip mismatch (n={}, first diff at {:?})",
            codec.name(),
            list.len(),
            back.iter().zip(list).position(|(a, b)| a != b).or(Some(back.len().min(list.len())))
        ));
    }
    let prepared = prepare(codec, kind, universe, list.len(), &buf);
    if kind == Kind::Sorted
        && let Some(mut cur) = prepared.cursor()
    {
        // Seek to every element via the gap just before it, then re-seek to
        // the element itself: the cursor must stay put.
        for (i, &v) in list.iter().enumerate() {
            let target = if i > 0 && list[i - 1] + 1 < v { list[i - 1] + 1 } else { v };
            if cur.next_geq(target) != Some(v) {
                return Err(format!("{}: next_geq({target}) != {v}", codec.name()));
            }
            if cur.next_geq(v) != Some(v) {
                return Err(format!("{}: next_geq({v}) moved off the current element", codec.name()));
            }
        }
        if cur.next_geq(universe).is_some() {
            return Err(format!("{}: next_geq(universe) should exhaust", codec.name()));
        }
        // Sequential next() from a fresh cursor.
        let Some(mut cur) = prepared.cursor() else {
            return Err(format!("{}: cursor disappeared", codec.name()));
        };
        for &v in list {
            if cur.next() != Some(v) {
                return Err(format!("{}: next() != {v}", codec.name()));
            }
        }
    }
    if codec.caps().random_access {
        for (i, &v) in list.iter().enumerate() {
            if prepared.get(i) != Some(v) {
                return Err(format!("{}: get({i}) != {v}", codec.name()));
            }
        }
    }
    Ok(())
}

/// Lists every adapter's unit tests run through [`check`]: empty, singleton,
/// dense run, sparse, a block-boundary straddler, and max-value edge cases.
#[cfg(test)]
pub fn conformance_cases(kind: Kind) -> Vec<(u32, Vec<u32>)> {
    let universe = 1 << 20;
    let mut cases = vec![
        (universe, vec![]),
        (universe, vec![0]),
        (universe, vec![universe - 1]),
        (universe, (0..300).collect()),
        (universe, (0..300).map(|i| i * 1000).collect()),
        (universe, (0..129).map(|i| i * 3 + 1).collect()),
        (universe, (0..1000).map(|i| i * i).collect()),
        (universe, vec![0, universe - 1]),
    ];
    if kind == Kind::Unsorted {
        cases.push((u32::MAX, vec![u32::MAX, 0, u32::MAX, 1]));
        cases.push((u32::MAX, (0..500u32).map(|i| i.wrapping_mul(2_654_435_761) % 1000).collect()));
        cases.push((16, vec![3; 1000]));
    }
    cases
}

#[cfg(test)]
pub fn conformance(codec: &dyn Codec) {
    for (universe, list) in conformance_cases(Kind::Sorted) {
        if let Err(e) = check(codec, Kind::Sorted, universe, &list) {
            panic!("sorted: {e}");
        }
    }
    if !codec.caps().sorted_only {
        for (universe, list) in conformance_cases(Kind::Unsorted) {
            if let Err(e) = check(codec, Kind::Unsorted, universe, &list) {
                panic!("unsorted: {e}");
            }
        }
    }
}
