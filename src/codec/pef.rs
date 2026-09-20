//! Fixed-128 partitioned Elias-Fano over strictly increasing `u32` lists.
//!
//! The upper level is a plain Elias-Fano sequence of partition maxima. A
//! packed two-bit directory selects all-ones, bitmap, or Elias-Fano storage
//! for each partition, and a fixed-width packed offset directory addresses
//! the byte-aligned payloads. Queries read those structures directly from the
//! encoded byte slice; no select index or owning prepared form is built.

use crate::stream::Kind;

use super::{Caps, Codec, Cursor};

const PARTITION_SIZE: usize = 128;
const TYPE_ALL_ONES: u8 = 0;
const TYPE_BITMAP: u8 = 1;
const TYPE_ELIAS_FANO: u8 = 2;

pub struct Pef;

impl Codec for Pef {
    fn name(&self) -> &'static str {
        "pef"
    }

    fn caps(&self) -> Caps {
        Caps { sorted_only: true, streaming_encoder: false, seek: true, random_access: true }
    }

    fn encode(&self, _kind: Kind, universe: u32, list: &[u32], out: &mut Vec<u8>) {
        if list.is_empty() {
            return;
        }

        let partition_count = list.len().div_ceil(PARTITION_SIZE);
        let upper_l = low_width(u64::from(universe), partition_count);
        let upper_bounds: Vec<u32> =
            list.chunks(PARTITION_SIZE).filter_map(|partition| partition.last().copied()).collect();
        let (upper_lows, upper_highs) = encode_elias_fano(&upper_bounds, 0, u64::from(universe), upper_l);

        let mut types = vec![0; partition_count.div_ceil(4)];
        let mut offsets = Vec::with_capacity(partition_count);
        let mut payload = Vec::new();
        let mut start = 0u64;
        for (partition_index, partition) in list.chunks(PARTITION_SIZE).enumerate() {
            offsets.push(payload.len() as u64);
            let hi = u64::from(partition[partition.len() - 1]);
            let local_universe = hi + 1 - start;
            let ef_l = low_width(local_universe, partition.len());
            let bitmap_bytes = (local_universe as usize).div_ceil(8);
            let ef_bytes = (partition.len() * usize::from(ef_l)).div_ceil(8)
                + (partition.len() + (local_universe >> ef_l) as usize).div_ceil(8);
            let partition_type = if local_universe == partition.len() as u64 {
                TYPE_ALL_ONES
            } else if bitmap_bytes <= ef_bytes {
                TYPE_BITMAP
            } else {
                TYPE_ELIAS_FANO
            };
            types[partition_index / 4] |= partition_type << ((partition_index % 4) * 2);

            match partition_type {
                TYPE_ALL_ONES => {}
                TYPE_BITMAP => encode_bitmap(partition, start, local_universe as usize, &mut payload),
                TYPE_ELIAS_FANO => {
                    let (lows, highs) = encode_elias_fano(partition, start, local_universe, ef_l);
                    payload.extend_from_slice(&lows);
                    payload.extend_from_slice(&highs);
                }
                _ => unreachable!(),
            }
            start = hi + 1;
        }

        let offset_width = bits_required(payload.len() as u64);
        let mut packed_offsets = BitWriter::default();
        for offset in offsets {
            packed_offsets.push(offset, offset_width);
        }

        out.push(upper_l);
        out.push(offset_width);
        out.extend_from_slice(&upper_lows);
        out.extend_from_slice(&upper_highs);
        out.extend_from_slice(&types);
        out.extend_from_slice(&packed_offsets.finish());
        out.extend_from_slice(&payload);
    }

    fn decode(&self, _kind: Kind, universe: u32, n: usize, buf: &[u8], out: &mut Vec<u32>) {
        if n == 0 {
            return;
        }
        let layout = Layout::new(universe, n, buf);
        for partition_index in 0..layout.partition_count {
            let partition = layout.partition(partition_index);
            let mut position = partition.position_at(0);
            while let Some(current) = position {
                out.push((partition.start + current.local as u64) as u32);
                position = partition.next_after(&current);
            }
        }
    }

    fn cursor<'a>(&self, universe: u32, n: usize, buf: &'a [u8]) -> Option<Box<dyn Cursor + 'a>> {
        Some(Box::new(PefCursor::new(universe, n, buf)))
    }

    fn get(&self, _kind: Kind, universe: u32, n: usize, buf: &[u8], index: usize) -> Option<u32> {
        if index >= n {
            return None;
        }
        let layout = Layout::new(universe, n, buf);
        layout.partition(index / PARTITION_SIZE).value_at(index % PARTITION_SIZE)
    }
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    bit_len: usize,
}

impl BitWriter {
    fn push(&mut self, mut value: u64, mut width: u8) {
        while width > 0 {
            let byte_index = self.bit_len / 8;
            let bit_index = self.bit_len % 8;
            if byte_index == self.bytes.len() {
                self.bytes.push(0);
            }
            let take = usize::from(width).min(8 - bit_index);
            let mask = (1u16 << take) - 1;
            self.bytes[byte_index] |= ((value & u64::from(mask)) as u8) << bit_index;
            value >>= take;
            width -= take as u8;
            self.bit_len += take;
        }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

fn bits_required(value: u64) -> u8 {
    (u64::BITS - value.leading_zeros()) as u8
}

fn low_width(universe: u64, count: usize) -> u8 {
    if count == 0 {
        return 0;
    }
    let quotient = universe / count as u64;
    bits_required(quotient).saturating_sub(1)
}

fn encode_elias_fano(values: &[u32], start: u64, universe: u64, low_width: u8) -> (Vec<u8>, Vec<u8>) {
    let mut lows = BitWriter::default();
    let high_bit_len = values.len() + (universe >> low_width) as usize;
    let mut highs = vec![0u8; high_bit_len.div_ceil(8)];
    let low_mask = if low_width == 0 { 0 } else { (1u64 << low_width) - 1 };
    for (index, &value) in values.iter().enumerate() {
        let local = u64::from(value) - start;
        lows.push(local & low_mask, low_width);
        set_bit(&mut highs, (local >> low_width) as usize + index);
    }
    (lows.finish(), highs)
}

fn encode_bitmap(values: &[u32], start: u64, universe: usize, out: &mut Vec<u8>) {
    let offset = out.len();
    out.resize(offset + universe.div_ceil(8), 0);
    for &value in values {
        set_bit(&mut out[offset..], (u64::from(value) - start) as usize);
    }
}

fn set_bit(bytes: &mut [u8], bit: usize) {
    bytes[bit / 8] |= 1 << (bit % 8);
}

fn read_bits(bytes: &[u8], bit_offset: usize, width: u8) -> u64 {
    let mut value = 0u64;
    let mut read = 0usize;
    while read < usize::from(width) {
        let absolute = bit_offset + read;
        let bit_index = absolute % 8;
        let take = (usize::from(width) - read).min(8 - bit_index);
        let mask = ((1u16 << take) - 1) as u8;
        value |= u64::from((bytes[absolute / 8] >> bit_index) & mask) << read;
        read += take;
    }
    value
}

fn select_one(bytes: &[u8], bit_len: usize, mut rank: usize) -> Option<usize> {
    for (word_index, chunk) in bytes.chunks(8).enumerate() {
        let mut word_bytes = [0u8; 8];
        word_bytes[..chunk.len()].copy_from_slice(chunk);
        let mut word = u64::from_le_bytes(word_bytes);
        if word_index * 64 + 64 > bit_len {
            word &= trailing_mask(bit_len.saturating_sub(word_index * 64));
        }
        let ones = word.count_ones() as usize;
        if rank < ones {
            for _ in 0..rank {
                word &= word - 1;
            }
            return Some(word_index * 64 + word.trailing_zeros() as usize);
        }
        rank -= ones;
    }
    None
}

fn find_next_one(bytes: &[u8], bit_len: usize, from: usize) -> Option<usize> {
    if from >= bit_len {
        return None;
    }
    let mut word_index = from / 64;
    let mut first = true;
    while word_index * 64 < bit_len {
        let byte_start = word_index * 8;
        let byte_end = (byte_start + 8).min(bytes.len());
        let mut word_bytes = [0u8; 8];
        word_bytes[..byte_end - byte_start].copy_from_slice(&bytes[byte_start..byte_end]);
        let mut word = u64::from_le_bytes(word_bytes);
        if first {
            word &= u64::MAX << (from % 64);
            first = false;
        }
        if word_index * 64 + 64 > bit_len {
            word &= trailing_mask(bit_len - word_index * 64);
        }
        if word != 0 {
            return Some(word_index * 64 + word.trailing_zeros() as usize);
        }
        word_index += 1;
    }
    None
}

fn trailing_mask(bits: usize) -> u64 {
    if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 }
}

struct Layout<'a> {
    buf: &'a [u8],
    n: usize,
    partition_count: usize,
    upper_l: u8,
    upper_low_start: usize,
    upper_high_start: usize,
    upper_high_bits: usize,
    type_start: usize,
    offset_start: usize,
    offset_width: u8,
    payload_start: usize,
}

impl<'a> Layout<'a> {
    fn new(universe: u32, n: usize, buf: &'a [u8]) -> Self {
        let partition_count = n.div_ceil(PARTITION_SIZE);
        if n == 0 {
            return Self {
                buf,
                n,
                partition_count,
                upper_l: 0,
                upper_low_start: 0,
                upper_high_start: 0,
                upper_high_bits: 0,
                type_start: 0,
                offset_start: 0,
                offset_width: 0,
                payload_start: 0,
            };
        }
        let upper_l = buf[0];
        let offset_width = buf[1];
        let upper_low_start = 2;
        let upper_low_bytes = (partition_count * usize::from(upper_l)).div_ceil(8);
        let upper_high_start = upper_low_start + upper_low_bytes;
        let upper_high_bits = partition_count + ((u64::from(universe) >> upper_l) as usize);
        let type_start = upper_high_start + upper_high_bits.div_ceil(8);
        let offset_start = type_start + partition_count.div_ceil(4);
        let payload_start = offset_start + (partition_count * usize::from(offset_width)).div_ceil(8);
        Self {
            buf,
            n,
            partition_count,
            upper_l,
            upper_low_start,
            upper_high_start,
            upper_high_bits,
            type_start,
            offset_start,
            offset_width,
            payload_start,
        }
    }

    fn upper_bound(&self, partition_index: usize) -> u32 {
        let position =
            select_one(&self.buf[self.upper_high_start..self.type_start], self.upper_high_bits, partition_index)
                .unwrap_or(0);
        self.upper_bound_at(partition_index, position)
    }

    fn upper_bound_at(&self, partition_index: usize, high_position: usize) -> u32 {
        let high = high_position - partition_index;
        let low = read_bits(
            &self.buf[self.upper_low_start..self.upper_high_start],
            partition_index * usize::from(self.upper_l),
            self.upper_l,
        );
        ((high as u64) << self.upper_l | low) as u32
    }

    fn lower_bound_partition(&self, target: u32, from: usize) -> Option<usize> {
        let mut low = from;
        let mut high = self.partition_count;
        while low < high {
            let middle = low + (high - low) / 2;
            if self.upper_bound(middle) < target {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        (low < self.partition_count).then_some(low)
    }

    fn partition(&self, partition_index: usize) -> Partition<'a> {
        count_partition_decode();
        let upper_highs = &self.buf[self.upper_high_start..self.type_start];
        let (start, hi) = if partition_index == 0 {
            (0, u64::from(self.upper_bound(0)))
        } else {
            let previous_position = select_one(upper_highs, self.upper_high_bits, partition_index - 1).unwrap_or(0);
            let previous = self.upper_bound_at(partition_index - 1, previous_position);
            let position =
                find_next_one(upper_highs, self.upper_high_bits, previous_position + 1).unwrap_or(previous_position);
            (u64::from(previous) + 1, u64::from(self.upper_bound_at(partition_index, position)))
        };
        let universe = (hi + 1 - start) as usize;
        let len = (self.n - partition_index * PARTITION_SIZE).min(PARTITION_SIZE);
        let partition_type = (self.buf[self.type_start + partition_index / 4] >> ((partition_index % 4) * 2)) & 3;
        let payload_offset = read_bits(
            &self.buf[self.offset_start..self.payload_start],
            partition_index * usize::from(self.offset_width),
            self.offset_width,
        ) as usize;
        let payload = &self.buf[self.payload_start + payload_offset..];
        let data = match partition_type {
            TYPE_ALL_ONES => PartitionData::AllOnes,
            TYPE_BITMAP => PartitionData::Bitmap(&payload[..universe.div_ceil(8)]),
            TYPE_ELIAS_FANO => {
                let low_width = low_width(universe as u64, len);
                let low_bytes = (len * usize::from(low_width)).div_ceil(8);
                let high_bits = len + (universe >> low_width);
                PartitionData::EliasFano {
                    lows: &payload[..low_bytes],
                    highs: &payload[low_bytes..low_bytes + high_bits.div_ceil(8)],
                    low_width,
                    high_bits,
                }
            }
            _ => unreachable!(),
        };
        Partition { start, len, universe, data }
    }
}

enum PartitionData<'a> {
    AllOnes,
    Bitmap(&'a [u8]),
    EliasFano { lows: &'a [u8], highs: &'a [u8], low_width: u8, high_bits: usize },
}

struct Partition<'a> {
    start: u64,
    len: usize,
    universe: usize,
    data: PartitionData<'a>,
}

struct Position {
    index: usize,
    local: usize,
    high_position: usize,
}

impl Partition<'_> {
    fn value_at(&self, index: usize) -> Option<u32> {
        if index >= self.len {
            return None;
        }
        self.position_at(index).map(|position| (self.start + position.local as u64) as u32)
    }

    fn position_at(&self, index: usize) -> Option<Position> {
        match &self.data {
            PartitionData::AllOnes => Some(Position { index, local: index, high_position: 0 }),
            PartitionData::Bitmap(bits) => {
                let local = select_one(bits, self.universe, index)?;
                Some(Position { index, local, high_position: 0 })
            }
            PartitionData::EliasFano { lows, highs, low_width, high_bits } => {
                let high_position = select_one(highs, *high_bits, index)?;
                let high = high_position - index;
                let low = read_bits(lows, index * usize::from(*low_width), *low_width) as usize;
                Some(Position { index, local: (high << *low_width) | low, high_position })
            }
        }
    }

    fn first_geq(&self, target: usize, from_index: usize) -> Option<Position> {
        match &self.data {
            PartitionData::AllOnes => {
                let local = target.max(from_index);
                (local < self.len).then_some(Position { index: local, local, high_position: 0 })
            }
            PartitionData::Bitmap(bits) => {
                let local = find_next_one(bits, self.universe, target)?;
                let index = count_ones_before(bits, local);
                (index >= from_index).then_some(Position { index, local, high_position: 0 })
            }
            PartitionData::EliasFano { lows, highs, low_width, high_bits } => {
                find_ef_geq(lows, highs, *low_width, *high_bits, target, from_index)
            }
        }
    }

    fn next_after(&self, current: &Position) -> Option<Position> {
        let index = current.index + 1;
        if index >= self.len {
            return None;
        }
        match &self.data {
            PartitionData::AllOnes => Some(Position { index, local: current.local + 1, high_position: 0 }),
            PartitionData::Bitmap(bits) => {
                let local = find_next_one(bits, self.universe, current.local + 1)?;
                Some(Position { index, local, high_position: 0 })
            }
            PartitionData::EliasFano { lows, highs, low_width, high_bits } => {
                let high_position = find_next_one(highs, *high_bits, current.high_position + 1)?;
                let high = high_position - index;
                let low = read_bits(lows, index * usize::from(*low_width), *low_width) as usize;
                Some(Position { index, local: (high << *low_width) | low, high_position })
            }
        }
    }
}

fn find_ef_geq(
    lows: &[u8],
    highs: &[u8],
    low_width: u8,
    high_bits: usize,
    target: usize,
    from_index: usize,
) -> Option<Position> {
    let target_high = target >> low_width;
    let mut index = 0usize;
    for (word_index, chunk) in highs.chunks(8).enumerate() {
        let mut word_bytes = [0u8; 8];
        word_bytes[..chunk.len()].copy_from_slice(chunk);
        let mut word = u64::from_le_bytes(word_bytes);
        if word_index * 64 + 64 > high_bits {
            word &= trailing_mask(high_bits - word_index * 64);
        }
        let ones = word.count_ones() as usize;
        if index + ones <= from_index {
            index += ones;
            continue;
        }
        if word != 0 {
            let last_position = word_index * 64 + (u64::BITS - 1 - word.leading_zeros()) as usize;
            let last_high = last_position - (index + ones - 1);
            if last_high < target_high {
                index += ones;
                continue;
            }
        }
        while word != 0 {
            let high_position = word_index * 64 + word.trailing_zeros() as usize;
            if index >= from_index {
                let high = high_position - index;
                let low = read_bits(lows, index * usize::from(low_width), low_width) as usize;
                let local = (high << low_width) | low;
                if local >= target {
                    return Some(Position { index, local, high_position });
                }
            }
            index += 1;
            word &= word - 1;
        }
    }
    None
}

fn count_ones_before(bytes: &[u8], bit: usize) -> usize {
    let full_bytes = bit / 8;
    let mut count: usize = bytes[..full_bytes].iter().map(|byte| byte.count_ones() as usize).sum();
    if !bit.is_multiple_of(8) {
        count += (bytes[full_bytes] & ((1 << (bit % 8)) - 1)).count_ones() as usize;
    }
    count
}

struct PefCursor<'a> {
    layout: Layout<'a>,
    partition_index: usize,
    partition: Option<Partition<'a>>,
    upper_high_position: usize,
    position: Option<Position>,
    current: Option<u32>,
}

impl<'a> PefCursor<'a> {
    fn new(universe: u32, n: usize, buf: &'a [u8]) -> Self {
        Self {
            layout: Layout::new(universe, n, buf),
            partition_index: usize::MAX,
            partition: None,
            upper_high_position: 0,
            position: None,
            current: None,
        }
    }

    fn load_partition(&mut self, partition_index: usize) {
        let upper_highs = &self.layout.buf[self.layout.upper_high_start..self.layout.type_start];
        self.upper_high_position = select_one(upper_highs, self.layout.upper_high_bits, partition_index).unwrap_or(0);
        self.load_partition_at(partition_index);
    }

    fn load_partition_at(&mut self, partition_index: usize) {
        self.partition_index = partition_index;
        self.partition = Some(self.layout.partition(partition_index));
        self.position = None;
        self.current = None;
    }

    fn partition_for_target(&mut self, target: u32) -> Option<usize> {
        if self.partition_index == usize::MAX {
            let partition_index = self.layout.lower_bound_partition(target, 0)?;
            self.load_partition(partition_index);
            return Some(partition_index);
        }
        let partition = self.partition.as_ref()?;
        if partition.start + partition.universe as u64 > u64::from(target) {
            return Some(self.partition_index);
        }

        let upper_highs = &self.layout.buf[self.layout.upper_high_start..self.layout.type_start];
        let mut partition_index = self.partition_index;
        let mut high_position = self.upper_high_position;
        while partition_index + 1 < self.layout.partition_count {
            partition_index += 1;
            high_position = find_next_one(upper_highs, self.layout.upper_high_bits, high_position + 1)?;
            if self.layout.upper_bound_at(partition_index, high_position) >= target {
                self.upper_high_position = high_position;
                self.load_partition_at(partition_index);
                return Some(partition_index);
            }
        }
        None
    }

    fn set_position(&mut self, position: Position) -> Option<u32> {
        let partition = self.partition.as_ref()?;
        let value = (partition.start + position.local as u64) as u32;
        self.position = Some(position);
        self.current = Some(value);
        Some(value)
    }

    fn exhaust(&mut self) -> Option<u32> {
        self.partition_index = self.layout.partition_count;
        self.partition = None;
        self.position = None;
        self.current = None;
        None
    }
}

impl Cursor for PefCursor<'_> {
    fn next_geq(&mut self, target: u32) -> Option<u32> {
        if self.current.is_some_and(|current| current >= target) {
            return self.current;
        }
        if self.layout.partition_count == 0 || self.partition_index == self.layout.partition_count {
            return None;
        }

        let Some(partition_index) = self.partition_for_target(target) else {
            return self.exhaust();
        };
        let partition = self.partition.as_ref()?;
        let local_target = u64::from(target).saturating_sub(partition.start) as usize;
        let from_index = self.position.as_ref().map_or(0, |position| position.index);
        if let Some(position) = partition.first_geq(local_target, from_index) {
            return self.set_position(position);
        }

        let next_partition = partition_index + 1;
        if next_partition >= self.layout.partition_count {
            return self.exhaust();
        }
        self.load_partition(next_partition);
        let position = self.partition.as_ref()?.position_at(0)?;
        self.set_position(position)
    }

    fn next(&mut self) -> Option<u32> {
        if self.layout.partition_count == 0 || self.partition_index == self.layout.partition_count {
            return None;
        }
        if self.partition_index == usize::MAX {
            self.load_partition(0);
            let position = self.partition.as_ref()?.position_at(0)?;
            return self.set_position(position);
        }
        if let (Some(partition), Some(current)) = (&self.partition, &self.position)
            && let Some(position) = partition.next_after(current)
        {
            return self.set_position(position);
        }
        let next_partition = self.partition_index + 1;
        if next_partition >= self.layout.partition_count {
            return self.exhaust();
        }
        self.load_partition(next_partition);
        let position = self.partition.as_ref()?.position_at(0)?;
        self.set_position(position)
    }
}

#[cfg(test)]
thread_local! {
    static PARTITION_DECODES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn count_partition_decode() {
    PARTITION_DECODES.with(|count| count.set(count.get() + 1));
}

#[cfg(not(test))]
fn count_partition_decode() {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::codec::raw::Raw;
    use crate::codec::{Codec, prepare};
    use crate::stream::{Kind, entropy_bits};

    use super::{Layout, PARTITION_DECODES, Pef, TYPE_ALL_ONES, TYPE_BITMAP, TYPE_ELIAS_FANO};

    fn encode(list: &[u32], universe: u32) -> Vec<u8> {
        let mut encoded = Vec::new();
        Pef.encode(Kind::Sorted, universe, list, &mut encoded);
        encoded
    }

    fn roundtrip(list: &[u32], universe: u32) {
        let encoded = encode(list, universe);
        let mut decoded = Vec::new();
        Pef.decode(Kind::Sorted, universe, list.len(), &encoded, &mut decoded);
        assert_eq!(decoded, list);
        for (index, &value) in list.iter().enumerate() {
            assert_eq!(Pef.get(Kind::Sorted, universe, list.len(), &encoded, index), Some(value));
        }
        assert_eq!(Pef.get(Kind::Sorted, universe, list.len(), &encoded, list.len()), None);
    }

    #[test]
    fn conformance() {
        super::super::conformance(&Pef);
    }

    #[test]
    fn all_partition_types_roundtrip_and_seek_like_raw() {
        let mut list: Vec<u32> = (0..=300).collect();
        list.extend((0..400).map(|index| 302 + index * 2));
        let sparse_start = list[list.len() - 1] + 10_000;
        list.extend((0..300).map(|index| sparse_start + index * 10_000));
        let universe = list[list.len() - 1] + 1;
        let encoded = encode(&list, universe);
        let layout = Layout::new(universe, list.len(), &encoded);
        let types: BTreeSet<u8> = (0..layout.partition_count)
            .map(|index| (encoded[layout.type_start + index / 4] >> ((index % 4) * 2)) & 3)
            .collect();
        assert_eq!(types, BTreeSet::from([TYPE_ALL_ONES, TYPE_BITMAP, TYPE_ELIAS_FANO]));
        roundtrip(&list, universe);

        let mut raw_bytes = Vec::new();
        Raw.encode(Kind::Sorted, universe, &list, &mut raw_bytes);
        let raw = prepare(&Raw, Kind::Sorted, universe, list.len(), &raw_bytes);
        let pef = prepare(&Pef, Kind::Sorted, universe, list.len(), &encoded);
        let Some(mut raw_cursor) = raw.cursor() else { panic!("raw cursor unavailable") };
        let Some(mut pef_cursor) = pef.cursor() else { panic!("PEF cursor unavailable") };
        let mut targets = Vec::with_capacity(list.len() * 2);
        for window in list.windows(2) {
            targets.push(window[0]);
            if window[1] - window[0] > 1 {
                targets.push(window[0] + (window[1] - window[0]) / 2);
            }
        }
        targets.push(list[list.len() - 1]);
        for target in targets {
            assert_eq!(pef_cursor.next_geq(target), raw_cursor.next_geq(target), "target {target}");
        }
    }

    #[test]
    fn seek_jumps_directly_to_partition() {
        let list: Vec<u32> = (0..128_000).map(|value| value * 10).collect();
        let universe = list[list.len() - 1] + 1;
        let encoded = encode(&list, universe);
        let Some(mut cursor) = Pef.cursor(universe, list.len(), &encoded) else { panic!("PEF cursor unavailable") };
        assert_eq!(cursor.next(), Some(0));
        PARTITION_DECODES.with(|count| count.set(0));
        assert_eq!(cursor.next_geq(list[900 * 128]), Some(list[900 * 128]));
        PARTITION_DECODES.with(|count| assert_eq!(count.get(), 1));
    }

    #[test]
    fn dense_and_boundary_lengths() {
        for n in [1, 128, 129, 100_000] {
            let list: Vec<u32> = (0..n as u32).collect();
            let encoded = encode(&list, n as u32);
            roundtrip(&list, n as u32);
            if n == 100_000 {
                assert!(encoded.len() as f64 * 8.0 / (n as f64) < 0.1);
            }
        }
    }

    #[test]
    fn size_sanity() {
        let every_second: Vec<u32> = (0..100_000).step_by(2).collect();
        let encoded = encode(&every_second, 100_000);
        let bits_per_int = encoded.len() as f64 * 8.0 / every_second.len() as f64;
        // Density 1/2 needs two bitmap bits per integer; directories add a
        // small amount. A 1.1-bit bound is impossible for this format.
        assert!(bits_per_int <= 2.25, "{bits_per_int} bits/int");

        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut values = BTreeSet::new();
        while values.len() < 10_000 {
            state ^= state << 7;
            state ^= state >> 9;
            state ^= state << 8;
            values.insert((state % 1_000_000_000) as u32);
        }
        let random: Vec<u32> = values.into_iter().collect();
        let encoded = encode(&random, 1_000_000_000);
        let bits_per_int = encoded.len() as f64 * 8.0 / random.len() as f64;
        let bound = entropy_bits(Kind::Sorted, 1_000_000_000, &random) / random.len() as f64;
        assert!(bits_per_int <= bound + 3.0, "{bits_per_int} vs bound {bound}");
    }
}
