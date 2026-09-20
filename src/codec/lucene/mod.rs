//! Lucene 10.3.1 block codecs, as a bench competitor.
//!
//! This slice is the block-level building blocks (`ForUtil`, `ForDeltaUtil`,
//! `PForUtil`, and the vint / group-vint primitives they sit on). A later
//! slice wraps them as a `Codec`. Formats are byte-exact against
//! `lucene-core-10.3.1`; see `lucene103-fixtures.tsv`.
//!
//! Values are `u32` and the arithmetic is unsigned. Java's `int` is signed
//! but Lucene only stores non-negative values, so every `>>>` is `>>` on
//! `u32`. Where Java would misbehave on values `>= 2^31` (signed compares in
//! `PForUtil.encode`), this port uses the natural unsigned extension.

pub mod for_delta;
pub mod for_util;
pub mod io;
pub mod pfor;

pub use for_util::BLOCK_SIZE;

#[cfg(test)]
mod tests {
    use super::{BLOCK_SIZE, for_delta, for_util, pfor};

    const FIXTURES: &str = include_str!("lucene103-fixtures.tsv");

    fn parse_hex(s: &str) -> Vec<u8> {
        assert!(s.len() % 2 == 0, "odd hex length");
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
            .collect()
    }

    fn parse_u32s(s: &str) -> Vec<u32> {
        if s.is_empty() {
            return Vec::new();
        }
        s.split(',').map(|t| t.parse::<u32>().expect("u32")).collect()
    }

    fn as_block(v: &[u32]) -> [u32; BLOCK_SIZE] {
        assert_eq!(v.len(), BLOCK_SIZE);
        let mut a = [0u32; BLOCK_SIZE];
        a.copy_from_slice(v);
        a
    }

    fn prefix(base: u32, deltas: &[u32; BLOCK_SIZE]) -> [u32; BLOCK_SIZE] {
        let mut out = [0u32; BLOCK_SIZE];
        let mut sum = base;
        for i in 0..BLOCK_SIZE {
            sum = sum.wrapping_add(deltas[i]);
            out[i] = sum;
        }
        out
    }

    #[test]
    fn lucene103_fixtures() {
        let mut n_for = 0;
        let mut n_delta = 0;
        let mut n_pfor = 0;
        let mut n_docs = 0;
        for (line_no, line) in FIXTURES.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            let mut cols = line.split('\t');
            let kind = cols.next().expect("kind");
            let bpv_s = cols.next().expect("bpv");
            let ints_s = cols.next().expect("ints");
            let hex_s = cols.next().expect("hex");
            let bytes = parse_hex(hex_s);
            match kind {
                "for" => {
                    let bpv: u32 = bpv_s.parse().expect("bpv");
                    let ints = as_block(&parse_u32s(ints_s));
                    let mut encoded = Vec::new();
                    for_util::encode(&ints, bpv, &mut encoded);
                    assert_eq!(encoded, bytes, "line {}: for encode bpv={bpv}", line_no + 1);
                    let mut decoded = [0u32; BLOCK_SIZE];
                    for_util::decode(bpv, &bytes, &mut decoded);
                    assert_eq!(decoded, ints, "line {}: for decode bpv={bpv}", line_no + 1);
                    n_for += 1;
                }
                "fordelta" => {
                    let bpv: u32 = bpv_s.parse().expect("bpv");
                    let deltas = as_block(&parse_u32s(ints_s));
                    assert_eq!(for_delta::bits_required(&deltas), bpv, "line {}: bits_required", line_no + 1);
                    let mut encoded = Vec::new();
                    for_delta::encode_deltas(bpv, &deltas, &mut encoded);
                    assert_eq!(encoded, bytes, "line {}: fordelta encode bpv={bpv}", line_no + 1);
                    for base in [0u32, 1_000_000] {
                        let mut decoded = [0u32; BLOCK_SIZE];
                        for_delta::decode_and_prefix_sum(bpv, &bytes, base, &mut decoded);
                        assert_eq!(
                            decoded,
                            prefix(base, &deltas),
                            "line {}: fordelta decode bpv={bpv} base={base}",
                            line_no + 1
                        );
                    }
                    n_delta += 1;
                }
                "pfor" => {
                    let ints = as_block(&parse_u32s(ints_s));
                    let mut encoded = Vec::new();
                    pfor::encode(&ints, &mut encoded);
                    assert_eq!(encoded, bytes, "line {}: pfor encode", line_no + 1);
                    let mut decoded = [0u32; BLOCK_SIZE];
                    let consumed = pfor::decode(&bytes, &mut decoded);
                    assert_eq!(consumed, bytes.len(), "line {}: pfor decode consumed", line_no + 1);
                    assert_eq!(decoded, ints, "line {}: pfor decode", line_no + 1);
                    assert_eq!(pfor::skip(&bytes), bytes.len(), "line {}: pfor skip", line_no + 1);
                    n_pfor += 1;
                }
                "docs" => n_docs += 1,
                other => panic!("line {}: unknown kind {other}", line_no + 1),
            }
        }
        assert_eq!((n_for, n_delta, n_pfor, n_docs), (64, 31, 28, 13));
    }
}

