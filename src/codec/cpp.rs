//! Lemire's C++ FastPFor library, via the `fastpfor` crate's `cpp` feature.
//! Whole-array codecs: a fresh C++ object is built per `encode`/`decode`
//! (instances are not thread-safe; construction is 0.1–0.8 µs, under the
//! 1 µs threshold where it would dominate the encode column on short lists).
//! Sorted lists are delta-coded like `fastpfor.rs`: first value as-is, then
//! `gap - 1`. The crate's `AnyLenCodec` writes a `u32` word stream and
//! truncates to the words actually used; those words are stored little-endian
//! with no slack.

use crate::stream::Kind;

use fastpfor::AnyLenCodec;

use super::{Caps, Codec};

const CAPS: Caps = Caps { sorted_only: false, streaming_encoder: false, seek: false, random_access: false };

/// First value as-is, subsequent `gap - 1` — the same convention as `fastpfor.rs`.
fn delta_gaps(list: &[u32]) -> Vec<u32> {
    let mut prev = 0u32;
    list.iter()
        .enumerate()
        .map(|(i, &v)| {
            let gap = if i == 0 { v } else { v - prev - 1 };
            prev = v;
            gap
        })
        .collect()
}

fn undelta_gaps(gaps: &[u32], out: &mut Vec<u32>) {
    let mut prev = 0u32;
    for (i, &gap) in gaps.iter().enumerate() {
        let v = if i == 0 { gap } else { prev + gap + 1 };
        out.push(v);
        prev = v;
    }
}

fn encode_any<C: AnyLenCodec>(kind: Kind, list: &[u32], out: &mut Vec<u8>) {
    let gaps;
    let input: &[u32] = match kind {
        Kind::Sorted => {
            gaps = delta_gaps(list);
            &gaps
        }
        Kind::Unsorted => list,
    };
    let mut words = Vec::new();
    let mut codec = C::default();
    if let Err(e) = codec.encode(input, &mut words) {
        panic!("C++ FastPFor encode failed: {e}");
    }
    out.reserve(words.len() * 4);
    for w in words {
        out.extend_from_slice(&w.to_le_bytes());
    }
}

fn decode_any<C: AnyLenCodec>(kind: Kind, n: usize, buf: &[u8], out: &mut Vec<u32>) {
    let words: Vec<u32> = buf.as_chunks::<4>().0.iter().map(|&c| u32::from_le_bytes(c)).collect();
    let mut gaps = Vec::new();
    let mut codec = C::default();
    if let Err(e) = codec.decode(&words, &mut gaps, Some(n as u32)) {
        panic!("C++ FastPFor decode failed: {e}");
    }
    match kind {
        Kind::Sorted => undelta_gaps(&gaps, out),
        Kind::Unsorted => out.extend_from_slice(&gaps),
    }
}

pub struct CppSimdFastPFor128;

impl Codec for CppSimdFastPFor128 {
    fn name(&self) -> &'static str {
        "cpp-simdfastpfor128"
    }
    fn caps(&self) -> Caps {
        CAPS
    }
    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        encode_any::<fastpfor::cpp::CppSimdFastPFor128>(kind, list, out);
    }
    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        decode_any::<fastpfor::cpp::CppSimdFastPFor128>(kind, n, buf, out);
    }
}

pub struct CppSimdBinaryPacking;

impl Codec for CppSimdBinaryPacking {
    fn name(&self) -> &'static str {
        "cpp-simdbinarypacking"
    }
    fn caps(&self) -> Caps {
        CAPS
    }
    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        encode_any::<fastpfor::cpp::CppSimdBinaryPacking>(kind, list, out);
    }
    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        decode_any::<fastpfor::cpp::CppSimdBinaryPacking>(kind, n, buf, out);
    }
}

pub struct CppOptPFor;

impl Codec for CppOptPFor {
    fn name(&self) -> &'static str {
        "cpp-optpfor"
    }
    fn caps(&self) -> Caps {
        CAPS
    }
    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        encode_any::<fastpfor::cpp::CppOptPFor>(kind, list, out);
    }
    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        decode_any::<fastpfor::cpp::CppOptPFor>(kind, n, buf, out);
    }
}

pub struct CppSimdPFor;

impl Codec for CppSimdPFor {
    fn name(&self) -> &'static str {
        "cpp-simdpfor"
    }
    fn caps(&self) -> Caps {
        CAPS
    }
    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        encode_any::<fastpfor::cpp::CppSimdPFor>(kind, list, out);
    }
    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        decode_any::<fastpfor::cpp::CppSimdPFor>(kind, n, buf, out);
    }
}

pub struct CppBP32;

impl Codec for CppBP32 {
    fn name(&self) -> &'static str {
        "cpp-bp32"
    }
    fn caps(&self) -> Caps {
        CAPS
    }
    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        encode_any::<fastpfor::cpp::CppBP32>(kind, list, out);
    }
    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        decode_any::<fastpfor::cpp::CppBP32>(kind, n, buf, out);
    }
}

#[cfg(test)]
mod tests {
    use crate::codec::Codec;
    use crate::stream::Kind;

    use super::super::conformance;

    #[test]
    fn conformance_simdfastpfor128() {
        conformance(&super::CppSimdFastPFor128);
    }

    #[test]
    fn conformance_simdbinarypacking() {
        conformance(&super::CppSimdBinaryPacking);
    }

    #[test]
    fn conformance_optpfor() {
        conformance(&super::CppOptPFor);
    }

    #[test]
    fn conformance_simdpfor() {
        conformance(&super::CppSimdPFor);
    }

    #[test]
    fn conformance_bp32() {
        conformance(&super::CppBP32);
    }

    #[test]
    fn rust_and_cpp_fastpfor128_both_roundtrip() {
        let list: Vec<u32> = (0..10_000).map(|i| i * 3).collect();
        let universe = match list.last() {
            Some(&v) => v + 1,
            None => 1,
        };
        let rust = crate::codec::fastpfor::FastPFor;
        let cpp = super::CppSimdFastPFor128;
        let mut rust_buf = Vec::new();
        rust.encode(Kind::Sorted, universe, &list, &mut rust_buf);
        let mut cpp_buf = Vec::new();
        cpp.encode(Kind::Sorted, universe, &list, &mut cpp_buf);
        let mut rust_back = Vec::new();
        rust.decode(Kind::Sorted, universe, list.len(), &rust_buf, &mut rust_back);
        let mut cpp_back = Vec::new();
        cpp.decode(Kind::Sorted, universe, list.len(), &cpp_buf, &mut cpp_back);
        assert_eq!(rust_back, list);
        assert_eq!(cpp_back, list);
        // Same wire format on this list (2684 bytes each); the harness does not
        // require that, but a sudden split would be worth noticing.
        assert_eq!(rust_buf, cpp_buf);
        assert_eq!(rust_buf.len(), 2684);
    }

    #[test]
    fn empty_list_is_one_header_word() {
        let codecs: [&dyn Codec; 5] = [
            &super::CppSimdFastPFor128,
            &super::CppSimdBinaryPacking,
            &super::CppOptPFor,
            &super::CppSimdPFor,
            &super::CppBP32,
        ];
        for c in codecs {
            let mut buf = Vec::new();
            c.encode(Kind::Sorted, 1, &[], &mut buf);
            assert_eq!(buf.len(), 4, "{}", c.name());
        }
    }
}
