//! Lucene 10.3.1 `ForUtil`: bit-pack 128 integers at a fixed bits-per-value.
//!
//! Primitive width follows bpv: 8-bit lanes at bpv ≤ 8, 16-bit at ≤ 16, else
//! 32-bit. Decode goes through [`split_ints`] then a leftover stitch for bpv
//! that do not divide the lane width. `decode(32, …)` is 128 raw little-endian
//! ints — Java's `decodeSlow(32)` would index `MASKS32[32]`.
#![cfg_attr(not(test), allow(dead_code))] // unused ForUtil helpers stay for the Java surface
#![allow(clippy::needless_range_loop)] // fixed-index packed-lane loops, for LLVM
#![allow(clippy::explicit_counter_loop)]

use super::io::{Reader, write_u32_le};

pub const BLOCK_SIZE: usize = 128;

/// PackedInts.bitsRequired: 1 for 0, else 32 − leading zeros.
pub(super) fn bits_required(x: u32) -> u32 {
    1.max(32 - x.leading_zeros())
}

pub fn num_bytes(bpv: u32) -> usize {
    (bpv as usize) << 4
}

const fn mask32(bits: u32) -> u32 {
    if bits == 0 {
        0
    } else if bits >= 32 {
        u32::MAX
    } else {
        (1u32 << bits) - 1
    }
}

const fn expand_mask16(mask16: u32) -> u32 {
    mask16 | (mask16 << 16)
}

const fn expand_mask8(mask8: u32) -> u32 {
    expand_mask16(mask8 | (mask8 << 8))
}

const fn mask16(bits: u32) -> u32 {
    expand_mask16(mask32(bits))
}

const fn mask8(bits: u32) -> u32 {
    expand_mask8(mask32(bits))
}

const MASKS8: [u32; 8] = {
    let mut m = [0u32; 8];
    let mut i = 0;
    while i < 8 {
        m[i] = mask8(i as u32);
        i += 1;
    }
    m
};
const MASKS16: [u32; 16] = {
    let mut m = [0u32; 16];
    let mut i = 0;
    while i < 16 {
        m[i] = mask16(i as u32);
        i += 1;
    }
    m
};
const MASKS32: [u32; 33] = {
    let mut m = [0u32; 33];
    let mut i = 0;
    while i < 33 {
        m[i] = mask32(i as u32);
        i += 1;
    }
    m
};

pub(super) const MASK8_1: u32 = mask8(1);
pub(super) const MASK8_2: u32 = mask8(2);
pub(super) const MASK8_3: u32 = mask8(3);
pub(super) const MASK8_4: u32 = mask8(4);
pub(super) const MASK8_5: u32 = mask8(5);
pub(super) const MASK8_6: u32 = mask8(6);
pub(super) const MASK8_7: u32 = mask8(7);
pub(super) const MASK16_1: u32 = mask16(1);
pub(super) const MASK16_2: u32 = mask16(2);
pub(super) const MASK16_3: u32 = mask16(3);
pub(super) const MASK16_4: u32 = mask16(4);
pub(super) const MASK16_5: u32 = mask16(5);
pub(super) const MASK16_6: u32 = mask16(6);
pub(super) const MASK16_7: u32 = mask16(7);
pub(super) const MASK16_8: u32 = mask16(8);
pub(super) const MASK16_9: u32 = mask16(9);
pub(super) const MASK16_10: u32 = mask16(10);
pub(super) const MASK16_11: u32 = mask16(11);
pub(super) const MASK16_12: u32 = mask16(12);
pub(super) const MASK16_13: u32 = mask16(13);
pub(super) const MASK16_14: u32 = mask16(14);
pub(super) const MASK16_15: u32 = mask16(15);
pub(super) const MASK32_1: u32 = mask32(1);
pub(super) const MASK32_2: u32 = mask32(2);
pub(super) const MASK32_3: u32 = mask32(3);
pub(super) const MASK32_4: u32 = mask32(4);
pub(super) const MASK32_5: u32 = mask32(5);
pub(super) const MASK32_6: u32 = mask32(6);
pub(super) const MASK32_7: u32 = mask32(7);
pub(super) const MASK32_8: u32 = mask32(8);
pub(super) const MASK32_9: u32 = mask32(9);
pub(super) const MASK32_10: u32 = mask32(10);
pub(super) const MASK32_11: u32 = mask32(11);
pub(super) const MASK32_12: u32 = mask32(12);
pub(super) const MASK32_13: u32 = mask32(13);
pub(super) const MASK32_14: u32 = mask32(14);
pub(super) const MASK32_15: u32 = mask32(15);
pub(super) const MASK32_16: u32 = mask32(16);

pub(super) fn collapse8(arr: &mut [u32; BLOCK_SIZE]) {
    for i in 0..32 {
        arr[i] = (arr[i] << 24) | (arr[32 + i] << 16) | (arr[64 + i] << 8) | arr[96 + i];
    }
}

pub(super) fn expand8(arr: &mut [u32; BLOCK_SIZE]) {
    for i in 0..32 {
        let l = arr[i];
        arr[i] = (l >> 24) & 0xff;
        arr[32 + i] = (l >> 16) & 0xff;
        arr[64 + i] = (l >> 8) & 0xff;
        arr[96 + i] = l & 0xff;
    }
}

pub(super) fn collapse16(arr: &mut [u32; BLOCK_SIZE]) {
    for i in 0..64 {
        arr[i] = (arr[i] << 16) | arr[64 + i];
    }
}

pub(super) fn expand16(arr: &mut [u32; BLOCK_SIZE]) {
    for i in 0..64 {
        let l = arr[i];
        arr[i] = (l >> 16) & 0xffff;
        arr[64 + i] = l & 0xffff;
    }
}

/// Encode 128 integers at `bpv` bits each. Copies `ints`; does not clobber them.
pub fn encode(ints: &[u32; BLOCK_SIZE], bpv: u32, out: &mut Vec<u8>) {
    let mut collapsed = *ints;
    let primitive = if bpv <= 8 {
        collapse8(&mut collapsed);
        8
    } else if bpv <= 16 {
        collapse16(&mut collapsed);
        16
    } else {
        32
    };
    encode_with_primitive(&collapsed, bpv, primitive, out);
}

pub(super) fn encode_with_primitive(ints: &[u32; BLOCK_SIZE], bpv: u32, primitive_size: u32, out: &mut Vec<u8>) {
    let num_ints = BLOCK_SIZE * primitive_size as usize / 32;
    let num_ints_per_shift = bpv as usize * 4;
    let mut tmp = [0u32; BLOCK_SIZE];
    let mut idx = 0usize;
    let mut shift = primitive_size as i32 - bpv as i32;
    for i in 0..num_ints_per_shift {
        tmp[i] = ints[idx] << (shift as u32);
        idx += 1;
    }
    shift -= bpv as i32;
    while shift >= 0 {
        for i in 0..num_ints_per_shift {
            tmp[i] |= ints[idx] << (shift as u32);
            idx += 1;
        }
        shift -= bpv as i32;
    }

    let remaining_bits_per_int = (shift + bpv as i32) as u32;
    let mask_remaining = mask_for(primitive_size, remaining_bits_per_int);

    let mut tmp_idx = 0usize;
    let mut remaining_bits_per_value = bpv;
    while idx < num_ints {
        if remaining_bits_per_value >= remaining_bits_per_int {
            remaining_bits_per_value -= remaining_bits_per_int;
            tmp[tmp_idx] |= (ints[idx] >> remaining_bits_per_value) & mask_remaining;
            tmp_idx += 1;
            if remaining_bits_per_value == 0 {
                idx += 1;
                remaining_bits_per_value = bpv;
            }
        } else {
            let mask1 = mask_for(primitive_size, remaining_bits_per_value);
            let mask2 = mask_for(primitive_size, remaining_bits_per_int - remaining_bits_per_value);
            tmp[tmp_idx] |= (ints[idx] & mask1) << (remaining_bits_per_int - remaining_bits_per_value);
            idx += 1;
            remaining_bits_per_value += bpv - remaining_bits_per_int;
            tmp[tmp_idx] |= (ints[idx] >> remaining_bits_per_value) & mask2;
            tmp_idx += 1;
        }
    }

    for i in 0..num_ints_per_shift {
        write_u32_le(out, tmp[i]);
    }
}

fn mask_for(primitive_size: u32, bits: u32) -> u32 {
    match primitive_size {
        8 => MASKS8[bits as usize],
        16 => MASKS16[bits as usize],
        _ => MASKS32[bits as usize],
    }
}

/// Decode 128 integers. `input` is at least [`num_bytes`] long; extra is ignored.
pub fn decode(bpv: u32, input: &[u8], out: &mut [u32; BLOCK_SIZE]) {
    let mut r = Reader::new(input);
    let mut tmp = [0u32; BLOCK_SIZE];
    match bpv {
        1 => {
            decode1(&mut r, out);
            expand8(out);
        }
        2 => {
            decode2(&mut r, out);
            expand8(out);
        }
        3 => {
            decode3(&mut r, &mut tmp, out);
            expand8(out);
        }
        4 => {
            decode4(&mut r, out);
            expand8(out);
        }
        5 => {
            decode5(&mut r, &mut tmp, out);
            expand8(out);
        }
        6 => {
            decode6(&mut r, &mut tmp, out);
            expand8(out);
        }
        7 => {
            decode7(&mut r, &mut tmp, out);
            expand8(out);
        }
        8 => {
            decode8(&mut r, out);
            expand8(out);
        }
        9 => {
            decode9(&mut r, &mut tmp, out);
            expand16(out);
        }
        10 => {
            decode10(&mut r, &mut tmp, out);
            expand16(out);
        }
        11 => {
            decode11(&mut r, &mut tmp, out);
            expand16(out);
        }
        12 => {
            decode12(&mut r, &mut tmp, out);
            expand16(out);
        }
        13 => {
            decode13(&mut r, &mut tmp, out);
            expand16(out);
        }
        14 => {
            decode14(&mut r, &mut tmp, out);
            expand16(out);
        }
        15 => {
            decode15(&mut r, &mut tmp, out);
            expand16(out);
        }
        16 => {
            decode16(&mut r, out);
            expand16(out);
        }
        _ => decode_slow(bpv, &mut r, &mut tmp, out),
    }
}

/// Port of `PostingDecodingUtil.splitInts` writing shift-bands and the
/// remainder into the same array (`c` at `c_index`).
#[allow(clippy::too_many_arguments)] // matches Lucene splitInts
pub(super) fn split_ints(
    input: &mut Reader<'_>,
    count: usize,
    arr: &mut [u32],
    b_shift: u32,
    dec: u32,
    b_mask: u32,
    c_index: usize,
    c_mask: u32,
) {
    let mut raw = [0u32; BLOCK_SIZE];
    for slot in raw.iter_mut().take(count) {
        *slot = input.read_u32_le();
    }
    split_from_raw(&raw, count, arr, b_shift, dec, b_mask);
    for i in 0..count {
        arr[c_index + i] = raw[i] & c_mask;
    }
}

/// `splitInts` with remainder in a separate `tmp` (cIndex = 0).
#[allow(clippy::too_many_arguments)] // matches Lucene splitInts
pub(super) fn split_ints_tmp(
    input: &mut Reader<'_>,
    count: usize,
    b: &mut [u32],
    b_shift: u32,
    dec: u32,
    b_mask: u32,
    tmp: &mut [u32],
    c_mask: u32,
) {
    let mut raw = [0u32; BLOCK_SIZE];
    for slot in raw.iter_mut().take(count) {
        *slot = input.read_u32_le();
    }
    split_from_raw(&raw, count, b, b_shift, dec, b_mask);
    for i in 0..count {
        tmp[i] = raw[i] & c_mask;
    }
}

fn split_from_raw(raw: &[u32; BLOCK_SIZE], count: usize, b: &mut [u32], b_shift: u32, dec: u32, b_mask: u32) {
    // Java (bShift - 1) / dec is signed towards-zero; b_shift == 0 (bpv 32) yields one iter at shift 0.
    let max_iter = (b_shift as i32 - 1) / (dec as i32);
    for j in 0..=max_iter {
        let shift = (b_shift as i32 - j * dec as i32) as u32;
        let b_offset = count * j as usize;
        for i in 0..count {
            b[b_offset + i] = shr(raw[i], shift) & b_mask;
        }
    }
}

fn shr(v: u32, shift: u32) -> u32 {
    if shift >= 32 { 0 } else { v >> shift }
}

pub(super) fn decode1(input: &mut Reader<'_>, ints: &mut [u32; BLOCK_SIZE]) {
    split_ints(input, 4, ints, 7, 1, MASK8_1, 28, MASK8_1);
}

pub(super) fn decode2(input: &mut Reader<'_>, ints: &mut [u32; BLOCK_SIZE]) {
    split_ints(input, 8, ints, 6, 2, MASK8_2, 24, MASK8_2);
}

pub(super) fn decode3(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 12, ints, 5, 3, MASK8_3, tmp, MASK8_2);
    let mut tmp_idx = 0;
    let mut ints_idx = 24;
    for _ in 0..4 {
        let mut l0 = tmp[tmp_idx] << 1;
        l0 |= (tmp[tmp_idx + 1] >> 1) & MASK8_1;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 1] & MASK8_1) << 2;
        l1 |= tmp[tmp_idx + 2];
        ints[ints_idx + 1] = l1;
        tmp_idx += 3;
        ints_idx += 2;
    }
}

fn decode4(input: &mut Reader<'_>, ints: &mut [u32; BLOCK_SIZE]) {
    split_ints(input, 16, ints, 4, 4, MASK8_4, 16, MASK8_4);
}

fn decode5(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 20, ints, 3, 5, MASK8_5, tmp, MASK8_3);
    let mut tmp_idx = 0;
    let mut ints_idx = 20;
    for _ in 0..4 {
        let mut l0 = tmp[tmp_idx] << 2;
        l0 |= (tmp[tmp_idx + 1] >> 1) & MASK8_2;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 1] & MASK8_1) << 4;
        l1 |= tmp[tmp_idx + 2] << 1;
        l1 |= (tmp[tmp_idx + 3] >> 2) & MASK8_1;
        ints[ints_idx + 1] = l1;
        let mut l2 = (tmp[tmp_idx + 3] & MASK8_2) << 3;
        l2 |= tmp[tmp_idx + 4];
        ints[ints_idx + 2] = l2;
        tmp_idx += 5;
        ints_idx += 3;
    }
}

fn decode6(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 24, ints, 2, 6, MASK8_6, tmp, MASK8_2);
    let mut tmp_idx = 0;
    let mut ints_idx = 24;
    for _ in 0..8 {
        let mut l0 = tmp[tmp_idx] << 4;
        l0 |= tmp[tmp_idx + 1] << 2;
        l0 |= tmp[tmp_idx + 2];
        ints[ints_idx] = l0;
        tmp_idx += 3;
        ints_idx += 1;
    }
}

fn decode7(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 28, ints, 1, 7, MASK8_7, tmp, MASK8_1);
    let mut tmp_idx = 0;
    let mut ints_idx = 28;
    for _ in 0..4 {
        let mut l0 = tmp[tmp_idx] << 6;
        l0 |= tmp[tmp_idx + 1] << 5;
        l0 |= tmp[tmp_idx + 2] << 4;
        l0 |= tmp[tmp_idx + 3] << 3;
        l0 |= tmp[tmp_idx + 4] << 2;
        l0 |= tmp[tmp_idx + 5] << 1;
        l0 |= tmp[tmp_idx + 6];
        ints[ints_idx] = l0;
        tmp_idx += 7;
        ints_idx += 1;
    }
}

fn decode8(input: &mut Reader<'_>, ints: &mut [u32; BLOCK_SIZE]) {
    for i in 0..32 {
        ints[i] = input.read_u32_le();
    }
}

pub(super) fn decode9(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 36, ints, 7, 9, MASK16_9, tmp, MASK16_7);
    let mut tmp_idx = 0;
    let mut ints_idx = 36;
    for _ in 0..4 {
        let mut l0 = tmp[tmp_idx] << 2;
        l0 |= (tmp[tmp_idx + 1] >> 5) & MASK16_2;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 1] & MASK16_5) << 4;
        l1 |= (tmp[tmp_idx + 2] >> 3) & MASK16_4;
        ints[ints_idx + 1] = l1;
        let mut l2 = (tmp[tmp_idx + 2] & MASK16_3) << 6;
        l2 |= (tmp[tmp_idx + 3] >> 1) & MASK16_6;
        ints[ints_idx + 2] = l2;
        let mut l3 = (tmp[tmp_idx + 3] & MASK16_1) << 8;
        l3 |= tmp[tmp_idx + 4] << 1;
        l3 |= (tmp[tmp_idx + 5] >> 6) & MASK16_1;
        ints[ints_idx + 3] = l3;
        let mut l4 = (tmp[tmp_idx + 5] & MASK16_6) << 3;
        l4 |= (tmp[tmp_idx + 6] >> 4) & MASK16_3;
        ints[ints_idx + 4] = l4;
        let mut l5 = (tmp[tmp_idx + 6] & MASK16_4) << 5;
        l5 |= (tmp[tmp_idx + 7] >> 2) & MASK16_5;
        ints[ints_idx + 5] = l5;
        let mut l6 = (tmp[tmp_idx + 7] & MASK16_2) << 7;
        l6 |= tmp[tmp_idx + 8];
        ints[ints_idx + 6] = l6;
        tmp_idx += 9;
        ints_idx += 7;
    }
}

pub(super) fn decode10(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 40, ints, 6, 10, MASK16_10, tmp, MASK16_6);
    let mut tmp_idx = 0;
    let mut ints_idx = 40;
    for _ in 0..8 {
        let mut l0 = tmp[tmp_idx] << 4;
        l0 |= (tmp[tmp_idx + 1] >> 2) & MASK16_4;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 1] & MASK16_2) << 8;
        l1 |= tmp[tmp_idx + 2] << 2;
        l1 |= (tmp[tmp_idx + 3] >> 4) & MASK16_2;
        ints[ints_idx + 1] = l1;
        let mut l2 = (tmp[tmp_idx + 3] & MASK16_4) << 6;
        l2 |= tmp[tmp_idx + 4];
        ints[ints_idx + 2] = l2;
        tmp_idx += 5;
        ints_idx += 3;
    }
}

fn decode11(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 44, ints, 5, 11, MASK16_11, tmp, MASK16_5);
    let mut tmp_idx = 0;
    let mut ints_idx = 44;
    for _ in 0..4 {
        let mut l0 = tmp[tmp_idx] << 6;
        l0 |= tmp[tmp_idx + 1] << 1;
        l0 |= (tmp[tmp_idx + 2] >> 4) & MASK16_1;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 2] & MASK16_4) << 7;
        l1 |= tmp[tmp_idx + 3] << 2;
        l1 |= (tmp[tmp_idx + 4] >> 3) & MASK16_2;
        ints[ints_idx + 1] = l1;
        let mut l2 = (tmp[tmp_idx + 4] & MASK16_3) << 8;
        l2 |= tmp[tmp_idx + 5] << 3;
        l2 |= (tmp[tmp_idx + 6] >> 2) & MASK16_3;
        ints[ints_idx + 2] = l2;
        let mut l3 = (tmp[tmp_idx + 6] & MASK16_2) << 9;
        l3 |= tmp[tmp_idx + 7] << 4;
        l3 |= (tmp[tmp_idx + 8] >> 1) & MASK16_4;
        ints[ints_idx + 3] = l3;
        let mut l4 = (tmp[tmp_idx + 8] & MASK16_1) << 10;
        l4 |= tmp[tmp_idx + 9] << 5;
        l4 |= tmp[tmp_idx + 10];
        ints[ints_idx + 4] = l4;
        tmp_idx += 11;
        ints_idx += 5;
    }
}

fn decode12(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 48, ints, 4, 12, MASK16_12, tmp, MASK16_4);
    let mut tmp_idx = 0;
    let mut ints_idx = 48;
    for _ in 0..16 {
        let mut l0 = tmp[tmp_idx] << 8;
        l0 |= tmp[tmp_idx + 1] << 4;
        l0 |= tmp[tmp_idx + 2];
        ints[ints_idx] = l0;
        tmp_idx += 3;
        ints_idx += 1;
    }
}

fn decode13(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 52, ints, 3, 13, MASK16_13, tmp, MASK16_3);
    let mut tmp_idx = 0;
    let mut ints_idx = 52;
    for _ in 0..4 {
        let mut l0 = tmp[tmp_idx] << 10;
        l0 |= tmp[tmp_idx + 1] << 7;
        l0 |= tmp[tmp_idx + 2] << 4;
        l0 |= tmp[tmp_idx + 3] << 1;
        l0 |= (tmp[tmp_idx + 4] >> 2) & MASK16_1;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 4] & MASK16_2) << 11;
        l1 |= tmp[tmp_idx + 5] << 8;
        l1 |= tmp[tmp_idx + 6] << 5;
        l1 |= tmp[tmp_idx + 7] << 2;
        l1 |= (tmp[tmp_idx + 8] >> 1) & MASK16_2;
        ints[ints_idx + 1] = l1;
        let mut l2 = (tmp[tmp_idx + 8] & MASK16_1) << 12;
        l2 |= tmp[tmp_idx + 9] << 9;
        l2 |= tmp[tmp_idx + 10] << 6;
        l2 |= tmp[tmp_idx + 11] << 3;
        l2 |= tmp[tmp_idx + 12];
        ints[ints_idx + 2] = l2;
        tmp_idx += 13;
        ints_idx += 3;
    }
}

fn decode14(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 56, ints, 2, 14, MASK16_14, tmp, MASK16_2);
    let mut tmp_idx = 0;
    let mut ints_idx = 56;
    for _ in 0..8 {
        let mut l0 = tmp[tmp_idx] << 12;
        l0 |= tmp[tmp_idx + 1] << 10;
        l0 |= tmp[tmp_idx + 2] << 8;
        l0 |= tmp[tmp_idx + 3] << 6;
        l0 |= tmp[tmp_idx + 4] << 4;
        l0 |= tmp[tmp_idx + 5] << 2;
        l0 |= tmp[tmp_idx + 6];
        ints[ints_idx] = l0;
        tmp_idx += 7;
        ints_idx += 1;
    }
}

fn decode15(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 60, ints, 1, 15, MASK16_15, tmp, MASK16_1);
    let mut tmp_idx = 0;
    let mut ints_idx = 60;
    for _ in 0..4 {
        let mut l0 = tmp[tmp_idx] << 14;
        l0 |= tmp[tmp_idx + 1] << 13;
        l0 |= tmp[tmp_idx + 2] << 12;
        l0 |= tmp[tmp_idx + 3] << 11;
        l0 |= tmp[tmp_idx + 4] << 10;
        l0 |= tmp[tmp_idx + 5] << 9;
        l0 |= tmp[tmp_idx + 6] << 8;
        l0 |= tmp[tmp_idx + 7] << 7;
        l0 |= tmp[tmp_idx + 8] << 6;
        l0 |= tmp[tmp_idx + 9] << 5;
        l0 |= tmp[tmp_idx + 10] << 4;
        l0 |= tmp[tmp_idx + 11] << 3;
        l0 |= tmp[tmp_idx + 12] << 2;
        l0 |= tmp[tmp_idx + 13] << 1;
        l0 |= tmp[tmp_idx + 14];
        ints[ints_idx] = l0;
        tmp_idx += 15;
        ints_idx += 1;
    }
}

fn decode16(input: &mut Reader<'_>, ints: &mut [u32; BLOCK_SIZE]) {
    for i in 0..64 {
        ints[i] = input.read_u32_le();
    }
}

pub(super) fn decode_slow(bpv: u32, input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    let num_ints = (bpv as usize) << 2;
    let mask = MASKS32[bpv as usize];
    split_ints_tmp(input, num_ints, ints, 32 - bpv, 32, mask, tmp, u32::MAX);
    let remaining_bits_per_int = 32 - bpv;
    let mask32_remaining = MASKS32[remaining_bits_per_int as usize];
    let mut tmp_idx = 0usize;
    let mut remaining_bits = remaining_bits_per_int;
    for ints_idx in num_ints..BLOCK_SIZE {
        let mut b = bpv - remaining_bits;
        let mut l = (tmp[tmp_idx] & MASKS32[remaining_bits as usize]) << b;
        tmp_idx += 1;
        while b >= remaining_bits_per_int {
            b -= remaining_bits_per_int;
            l |= (tmp[tmp_idx] & mask32_remaining) << b;
            tmp_idx += 1;
        }
        if b > 0 {
            l |= shr(tmp[tmp_idx], remaining_bits_per_int - b) & MASKS32[b as usize];
            remaining_bits = remaining_bits_per_int - b;
        } else {
            remaining_bits = remaining_bits_per_int;
        }
        ints[ints_idx] = l;
    }
}
