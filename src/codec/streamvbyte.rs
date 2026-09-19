//! `streamvbyte64` crate's `Coder1234`: byte-aligned variable-length coding
//! (1/2/3/4-byte values) in groups of four, tag stream followed by data
//! stream. The coder only handles group-aligned inputs, so lists are padded to
//! a multiple of four (with the last value for sorted lists, so the padding
//! deltas are zero) and the padding is discarded on decode. Sorted lists use
//! the crate's native delta mode with initial value 0 — it stores
//! `x[i] - x[i-1]` and represents a first element of 0 fine, so no `gap - 1`
//! trick is needed. The tag length is derivable from `n`, so no header is
//! written.

use crate::stream::Kind;

use streamvbyte64::{Coder, Coder1234};

use super::{Caps, Codec};

pub struct StreamVByte;

impl Codec for StreamVByte {
    fn name(&self) -> &'static str {
        "streamvbyte"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: false, streaming_encoder: true, seek: false, random_access: false }
    }

    fn encode(&self, kind: Kind, _universe: u32, list: &[u32], out: &mut Vec<u8>) {
        let coder = Coder1234::new();
        let n_groups = list.len().div_ceil(4);
        let padded_len = n_groups * 4;
        let mut padded = list.to_vec();
        let pad = list.last().copied().unwrap_or(0);
        padded.resize(padded_len, pad);
        let (tag_len, data_len) = Coder1234::max_compressed_bytes(padded_len);
        let mut buf = vec![0u8; tag_len + data_len];
        let (tags, data) = buf.split_at_mut(tag_len);
        let written = match kind {
            Kind::Sorted => coder.encode_deltas(0, &padded, tags, data),
            Kind::Unsorted => coder.encode(&padded, tags, data),
        };
        out.extend_from_slice(tags);
        out.extend_from_slice(&data[..written]);
    }

    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        let coder = Coder1234::new();
        let n_groups = n.div_ceil(4);
        let tags = &buf[..n_groups];
        let data = &buf[n_groups..];
        let mut values = vec![0u32; n_groups * 4];
        match kind {
            Kind::Sorted => {
                let _ = coder.decode_deltas(0, tags, data, &mut values);
            }
            Kind::Unsorted => {
                let _ = coder.decode(tags, data, &mut values);
            }
        }
        out.extend_from_slice(&values[..n]);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::StreamVByte);
    }
}
