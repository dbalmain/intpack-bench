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
        // Only copy when padding is actually needed; whole groups encode in place.
        let mut padded = Vec::new();
        let input: &[u32] = if padded_len == list.len() {
            list
        } else {
            padded.reserve(padded_len);
            padded.extend_from_slice(list);
            padded.resize(padded_len, list.last().copied().unwrap_or(0));
            &padded
        };
        let (tag_len, data_len) = Coder1234::max_compressed_bytes(padded_len);
        let start = out.len();
        out.resize(start + tag_len + data_len, 0);
        let (tags, data) = out[start..].split_at_mut(tag_len);
        let written = match kind {
            Kind::Sorted => coder.encode_deltas(0, input, tags, data),
            Kind::Unsorted => coder.encode(input, tags, data),
        };
        out.truncate(start + tag_len + written);
    }

    fn decode(&self, kind: Kind, _universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        let coder = Coder1234::new();
        let n_groups = n.div_ceil(4);
        let tags = &buf[..n_groups];
        let data = &buf[n_groups..];
        // Decode straight into `out` (padded to whole groups), then drop the padding.
        let start = out.len();
        out.resize(start + n_groups * 4, 0);
        match kind {
            Kind::Sorted => {
                let _ = coder.decode_deltas(0, tags, data, &mut out[start..]);
            }
            Kind::Unsorted => {
                let _ = coder.decode(tags, data, &mut out[start..]);
            }
        }
        out.truncate(start + n);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conformance() {
        super::super::conformance(&super::StreamVByte);
    }
}
