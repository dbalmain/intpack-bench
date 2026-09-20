//! Lucene `DataInput` / `DataOutput` primitives used by the 10.3.1 postings
//! format: little-endian int/long, vint/vlong, vint15/vlong15, group-varint.
#![cfg_attr(not(test), allow(dead_code))] // unused DataInput helpers stay for the Java surface

/// Cursor over a byte slice. Short reads yield zero and pin `pos` at the end
/// rather than panic: callers pass well-formed Lucene blocks.
pub struct Reader<'a> {
    pub buf: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn read_byte(&mut self) -> u8 {
        match self.buf.get(self.pos) {
            Some(&b) => {
                self.pos += 1;
                b
            }
            None => 0,
        }
    }

    pub fn read_u16_le(&mut self) -> u16 {
        let Some(rest) = self.buf.get(self.pos..) else { return 0 };
        let Some((chunk, _)) = rest.split_first_chunk::<2>() else {
            self.pos = self.buf.len();
            return 0;
        };
        self.pos += 2;
        u16::from_le_bytes(*chunk)
    }

    pub fn read_u32_le(&mut self) -> u32 {
        let Some(rest) = self.buf.get(self.pos..) else { return 0 };
        let Some((chunk, _)) = rest.split_first_chunk::<4>() else {
            self.pos = self.buf.len();
            return 0;
        };
        self.pos += 4;
        u32::from_le_bytes(*chunk)
    }

    pub fn read_u64_le(&mut self) -> u64 {
        let Some(rest) = self.buf.get(self.pos..) else { return 0 };
        let Some((chunk, _)) = rest.split_first_chunk::<8>() else {
            self.pos = self.buf.len();
            return 0;
        };
        self.pos += 8;
        u64::from_le_bytes(*chunk)
    }

    pub fn read_vint(&mut self) -> u32 {
        let mut v = 0u32;
        let mut shift = 0;
        loop {
            let b = self.read_byte();
            v |= u32::from(b & 0x7f) << shift;
            if b < 0x80 {
                return v;
            }
            shift += 7;
            if shift >= 32 {
                return v;
            }
        }
    }

    pub fn read_vlong(&mut self) -> u64 {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let b = self.read_byte();
            v |= u64::from(b & 0x7f) << shift;
            if b < 0x80 {
                return v;
            }
            shift += 7;
            if shift >= 64 {
                return v;
            }
        }
    }

    /// Little-endian `short`; top bit set means a vint of the rest follows.
    pub fn read_vint15(&mut self) -> u32 {
        let s = self.read_u16_le();
        if s < 0x8000 { u32::from(s) } else { u32::from(s & 0x7fff) | (self.read_vint() << 15) }
    }

    /// Little-endian `short`; top bit set means a vlong of the rest follows.
    pub fn read_vlong15(&mut self) -> u64 {
        let s = self.read_u16_le();
        if s < 0x8000 { u64::from(s) } else { u64::from(s & 0x7fff) | (self.read_vlong() << 15) }
    }

    /// Groups of 4: one flag byte of four 2-bit `bytes−1` widths, then the
    /// ints little-endian at that width. A trailing tail of `< 4` is vints.
    pub fn read_group_vints(&mut self, dst: &mut [u32]) {
        let mut i = 0;
        while i + 4 <= dst.len() {
            let flag = self.read_byte();
            dst[i] = self.read_group_int((flag >> 6) & 3);
            dst[i + 1] = self.read_group_int((flag >> 4) & 3);
            dst[i + 2] = self.read_group_int((flag >> 2) & 3);
            dst[i + 3] = self.read_group_int(flag & 3);
            i += 4;
        }
        for slot in dst.iter_mut().skip(i) {
            *slot = self.read_vint();
        }
    }

    fn read_group_int(&mut self, bytes_minus_1: u8) -> u32 {
        let n = usize::from(bytes_minus_1) + 1;
        let mut arr = [0u8; 4];
        for b in arr.iter_mut().take(n) {
            *b = self.read_byte();
        }
        u32::from_le_bytes(arr)
    }

    /// Advance `pos` by `n` bytes, pinning at EOF.
    pub fn skip(&mut self, n: usize) {
        self.pos = self.pos.saturating_add(n).min(self.buf.len());
    }
}

pub fn write_u32_le(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub fn write_u64_le(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub fn write_vint(out: &mut Vec<u8>, mut v: u32) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

pub fn write_vlong(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

pub fn write_vint15(out: &mut Vec<u8>, v: u32) {
    write_vlong15(out, u64::from(v));
}

pub fn write_vlong15(out: &mut Vec<u8>, v: u64) {
    if v & !0x7fff == 0 {
        out.extend_from_slice(&(v as u16).to_le_bytes());
    } else {
        let tagged = 0x8000u16 | (v as u16 & 0x7fff);
        out.extend_from_slice(&tagged.to_le_bytes());
        write_vlong(out, v >> 15);
    }
}

pub fn write_group_vints(out: &mut Vec<u8>, ints: &[u32]) {
    let mut i = 0;
    while ints.len() - i >= 4 {
        let n0 = group_bytes(ints[i]) - 1;
        let n1 = group_bytes(ints[i + 1]) - 1;
        let n2 = group_bytes(ints[i + 2]) - 1;
        let n3 = group_bytes(ints[i + 3]) - 1;
        out.push((n0 << 6) | (n1 << 4) | (n2 << 2) | n3);
        write_group_int(out, ints[i], n0 + 1);
        write_group_int(out, ints[i + 1], n1 + 1);
        write_group_int(out, ints[i + 2], n2 + 1);
        write_group_int(out, ints[i + 3], n3 + 1);
        i += 4;
    }
    for &v in &ints[i..] {
        write_vint(out, v);
    }
}

fn group_bytes(v: u32) -> u8 {
    // | 1 so that 0 still occupies one byte.
    4 - ((v | 1).leading_zeros() / 8) as u8
}

fn write_group_int(out: &mut Vec<u8>, v: u32, n: u8) {
    let bytes = v.to_le_bytes();
    out.extend_from_slice(&bytes[..usize::from(n)]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_vint(v: u32) {
        let mut buf = Vec::new();
        write_vint(&mut buf, v);
        let mut r = Reader::new(&buf);
        assert_eq!(r.read_vint(), v, "vint {v}");
        assert_eq!(r.pos, buf.len());
    }

    fn roundtrip_vlong(v: u64) {
        let mut buf = Vec::new();
        write_vlong(&mut buf, v);
        let mut r = Reader::new(&buf);
        assert_eq!(r.read_vlong(), v, "vlong {v}");
        assert_eq!(r.pos, buf.len());
    }

    #[test]
    fn le_int_long_roundtrip() {
        let mut buf = Vec::new();
        write_u32_le(&mut buf, 0x0102_0304);
        write_u64_le(&mut buf, 0x0807_0605_0403_0201);
        let mut r = Reader::new(&buf);
        assert_eq!(r.read_u32_le(), 0x0102_0304);
        assert_eq!(r.read_u64_le(), 0x0807_0605_0403_0201);
        assert_eq!(r.pos, buf.len());
    }

    #[test]
    fn vint_boundaries() {
        for v in [0, 127, 128, 16383, 16384, u32::MAX] {
            roundtrip_vint(v);
        }
    }

    #[test]
    fn vlong_boundaries() {
        for v in [0, 127, 128, 16383, 16384, u32::MAX as u64, (1u64 << 35) - 1, u64::MAX] {
            roundtrip_vlong(v);
        }
    }

    #[test]
    fn vint15_vlong15_boundaries() {
        for v in [0u32, 0x7fff, 0x8000, 0x8001, 1 << 20, u32::MAX] {
            let mut buf = Vec::new();
            write_vint15(&mut buf, v);
            let mut r = Reader::new(&buf);
            assert_eq!(r.read_vint15(), v, "vint15 {v}");
            assert_eq!(r.pos, buf.len());
        }
        for v in [0u64, 0x7fff, 0x8000, 0x8001, 1 << 20, u32::MAX as u64, 1u64 << 40, u64::MAX >> 1] {
            let mut buf = Vec::new();
            write_vlong15(&mut buf, v);
            let mut r = Reader::new(&buf);
            assert_eq!(r.read_vlong15(), v, "vlong15 {v}");
            assert_eq!(r.pos, buf.len());
        }
    }

    #[test]
    fn group_vint_lengths_0_to_9() {
        let samples: &[u32] = &[0, 0x7f, 0xff, 0x100, 0xffff, 0x1_0000, 0xff_ffff, 0x100_0000, u32::MAX];
        for n in 0..=9 {
            let ints: Vec<u32> = samples.iter().copied().cycle().take(n).collect();
            let mut buf = Vec::new();
            write_group_vints(&mut buf, &ints);
            let mut back = vec![0u32; n];
            let mut r = Reader::new(&buf);
            r.read_group_vints(&mut back);
            assert_eq!(back, ints, "group-vint n={n}");
            assert_eq!(r.pos, buf.len());
        }
    }

    #[test]
    fn group_vint_two_int_tail_matches_docs_fixture() {
        // Doc ids 3,9 with prevDocID = −1 are deltas [4, 6]. A tail of two is
        // plain vints — no flag byte. The first `docs` fixture line is `0406`.
        let mut buf = Vec::new();
        write_group_vints(&mut buf, &[4, 6]);
        assert_eq!(buf, [0x04, 0x06]);
    }
}
