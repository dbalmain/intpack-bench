//! Lucene 10.3.1 `ForDeltaUtil`: FOR of strictly-positive deltas, then a
//! prefix sum. Primitive-width thresholds (8 at bpv ≤ 3, 16 at ≤ 10, else 32)
//! differ from [`super::for_util`] because the prefix sum is done in SIMD-like
//! packed lanes and must not overflow a lane.

use super::for_util::{
    self, BLOCK_SIZE, MASK16_1, MASK16_2, MASK16_4, MASK16_5, MASK16_6, MASK16_7, MASK16_8, MASK32_1, MASK32_2,
    MASK32_3, MASK32_4, MASK32_5, MASK32_6, MASK32_7, MASK32_8, MASK32_9, MASK32_10, MASK32_11, MASK32_12, MASK32_13,
    MASK32_14, MASK32_15, MASK32_16, decode1, decode2, decode3, decode9, decode10, decode_slow, expand8, expand16,
    split_ints, split_ints_tmp,
};
use super::io::Reader;

pub fn bits_required(deltas: &[u32; BLOCK_SIZE]) -> u32 {
    let mut or = 0u32;
    for &d in deltas {
        or |= d;
    }
    for_util::bits_required(or)
}

pub fn encode_deltas(bpv: u32, deltas: &[u32; BLOCK_SIZE], out: &mut Vec<u8>) {
    let mut collapsed = *deltas;
    let primitive = if bpv <= 3 {
        for_util::collapse8(&mut collapsed);
        8
    } else if bpv <= 10 {
        for_util::collapse16(&mut collapsed);
        16
    } else {
        32
    };
    for_util::encode_with_primitive(&collapsed, bpv, primitive, out);
}

pub fn decode_and_prefix_sum(bpv: u32, input: &[u8], base: u32, out: &mut [u32; BLOCK_SIZE]) {
    let mut r = Reader::new(input);
    let mut tmp = [0u32; BLOCK_SIZE];
    match bpv {
        1 => {
            decode1(&mut r, out);
            prefix_sum8(out, base);
        }
        2 => {
            decode2(&mut r, out);
            prefix_sum8(out, base);
        }
        3 => {
            decode3(&mut r, &mut tmp, out);
            prefix_sum8(out, base);
        }
        4 => {
            decode4_to16(&mut r, out);
            prefix_sum16(out, base);
        }
        5 => {
            decode5_to16(&mut r, &mut tmp, out);
            prefix_sum16(out, base);
        }
        6 => {
            decode6_to16(&mut r, &mut tmp, out);
            prefix_sum16(out, base);
        }
        7 => {
            decode7_to16(&mut r, &mut tmp, out);
            prefix_sum16(out, base);
        }
        8 => {
            decode8_to16(&mut r, out);
            prefix_sum16(out, base);
        }
        9 => {
            decode9(&mut r, &mut tmp, out);
            prefix_sum16(out, base);
        }
        10 => {
            decode10(&mut r, &mut tmp, out);
            prefix_sum16(out, base);
        }
        11 => {
            decode11_to32(&mut r, &mut tmp, out);
            prefix_sum32(out, base);
        }
        12 => {
            decode12_to32(&mut r, &mut tmp, out);
            prefix_sum32(out, base);
        }
        13 => {
            decode13_to32(&mut r, &mut tmp, out);
            prefix_sum32(out, base);
        }
        14 => {
            decode14_to32(&mut r, &mut tmp, out);
            prefix_sum32(out, base);
        }
        15 => {
            decode15_to32(&mut r, &mut tmp, out);
            prefix_sum32(out, base);
        }
        16 => {
            decode16_to32(&mut r, out);
            prefix_sum32(out, base);
        }
        _ => {
            decode_slow(bpv, &mut r, &mut tmp, out);
            prefix_sum32(out, base);
        }
    }
}

fn prefix_sum(arr: &mut [u32], len: usize, base: u32) {
    let mut sum = base;
    for i in 0..len {
        sum = sum.wrapping_add(arr[i]);
        arr[i] = sum;
    }
}

fn prefix_sum8(arr: &mut [u32; BLOCK_SIZE], base: u32) {
    prefix_sum(arr, 32, 0);
    expand8(arr);
    let l0 = base;
    let l1 = l0.wrapping_add(arr[31]);
    let l2 = l1.wrapping_add(arr[63]);
    let l3 = l2.wrapping_add(arr[95]);
    for i in 0..32 {
        arr[i] = arr[i].wrapping_add(l0);
        arr[32 + i] = arr[32 + i].wrapping_add(l1);
        arr[64 + i] = arr[64 + i].wrapping_add(l2);
        arr[96 + i] = arr[96 + i].wrapping_add(l3);
    }
}

fn prefix_sum16(arr: &mut [u32; BLOCK_SIZE], base: u32) {
    prefix_sum(arr, 64, 0);
    expand16(arr);
    let l0 = base;
    let l1 = base.wrapping_add(arr[63]);
    for i in 0..64 {
        arr[i] = arr[i].wrapping_add(l0);
        arr[64 + i] = arr[64 + i].wrapping_add(l1);
    }
}

fn prefix_sum32(arr: &mut [u32; BLOCK_SIZE], base: u32) {
    prefix_sum(arr, BLOCK_SIZE, base);
}

fn decode4_to16(input: &mut Reader<'_>, ints: &mut [u32; BLOCK_SIZE]) {
    split_ints(input, 16, ints, 12, 4, MASK16_4, 48, MASK16_4);
}

fn decode5_to16(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 20, ints, 11, 5, MASK16_5, tmp, MASK16_1);
    let mut tmp_idx = 0;
    let mut ints_idx = 60;
    for _ in 0..4 {
        let mut l0 = tmp[tmp_idx] << 4;
        l0 |= tmp[tmp_idx + 1] << 3;
        l0 |= tmp[tmp_idx + 2] << 2;
        l0 |= tmp[tmp_idx + 3] << 1;
        l0 |= tmp[tmp_idx + 4];
        ints[ints_idx] = l0;
        tmp_idx += 5;
        ints_idx += 1;
    }
}

fn decode6_to16(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 24, ints, 10, 6, MASK16_6, tmp, MASK16_4);
    let mut tmp_idx = 0;
    let mut ints_idx = 48;
    for _ in 0..8 {
        let mut l0 = tmp[tmp_idx] << 2;
        l0 |= (tmp[tmp_idx + 1] >> 2) & MASK16_2;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 1] & MASK16_2) << 4;
        l1 |= tmp[tmp_idx + 2];
        ints[ints_idx + 1] = l1;
        tmp_idx += 3;
        ints_idx += 2;
    }
}

fn decode7_to16(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 28, ints, 9, 7, MASK16_7, tmp, MASK16_2);
    let mut tmp_idx = 0;
    let mut ints_idx = 56;
    for _ in 0..4 {
        let mut l0 = tmp[tmp_idx] << 5;
        l0 |= tmp[tmp_idx + 1] << 3;
        l0 |= tmp[tmp_idx + 2] << 1;
        l0 |= (tmp[tmp_idx + 3] >> 1) & MASK16_1;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 3] & MASK16_1) << 6;
        l1 |= tmp[tmp_idx + 4] << 4;
        l1 |= tmp[tmp_idx + 5] << 2;
        l1 |= tmp[tmp_idx + 6];
        ints[ints_idx + 1] = l1;
        tmp_idx += 7;
        ints_idx += 2;
    }
}

fn decode8_to16(input: &mut Reader<'_>, ints: &mut [u32; BLOCK_SIZE]) {
    split_ints(input, 32, ints, 8, 8, MASK16_8, 32, MASK16_8);
}

fn decode11_to32(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 44, ints, 21, 11, MASK32_11, tmp, MASK32_10);
    let mut tmp_idx = 0;
    let mut ints_idx = 88;
    for _ in 0..4 {
        let mut l0 = tmp[tmp_idx] << 1;
        l0 |= (tmp[tmp_idx + 1] >> 9) & MASK32_1;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 1] & MASK32_9) << 2;
        l1 |= (tmp[tmp_idx + 2] >> 8) & MASK32_2;
        ints[ints_idx + 1] = l1;
        let mut l2 = (tmp[tmp_idx + 2] & MASK32_8) << 3;
        l2 |= (tmp[tmp_idx + 3] >> 7) & MASK32_3;
        ints[ints_idx + 2] = l2;
        let mut l3 = (tmp[tmp_idx + 3] & MASK32_7) << 4;
        l3 |= (tmp[tmp_idx + 4] >> 6) & MASK32_4;
        ints[ints_idx + 3] = l3;
        let mut l4 = (tmp[tmp_idx + 4] & MASK32_6) << 5;
        l4 |= (tmp[tmp_idx + 5] >> 5) & MASK32_5;
        ints[ints_idx + 4] = l4;
        let mut l5 = (tmp[tmp_idx + 5] & MASK32_5) << 6;
        l5 |= (tmp[tmp_idx + 6] >> 4) & MASK32_6;
        ints[ints_idx + 5] = l5;
        let mut l6 = (tmp[tmp_idx + 6] & MASK32_4) << 7;
        l6 |= (tmp[tmp_idx + 7] >> 3) & MASK32_7;
        ints[ints_idx + 6] = l6;
        let mut l7 = (tmp[tmp_idx + 7] & MASK32_3) << 8;
        l7 |= (tmp[tmp_idx + 8] >> 2) & MASK32_8;
        ints[ints_idx + 7] = l7;
        let mut l8 = (tmp[tmp_idx + 8] & MASK32_2) << 9;
        l8 |= (tmp[tmp_idx + 9] >> 1) & MASK32_9;
        ints[ints_idx + 8] = l8;
        let mut l9 = (tmp[tmp_idx + 9] & MASK32_1) << 10;
        l9 |= tmp[tmp_idx + 10];
        ints[ints_idx + 9] = l9;
        tmp_idx += 11;
        ints_idx += 10;
    }
}

fn decode12_to32(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 48, ints, 20, 12, MASK32_12, tmp, MASK32_8);
    let mut tmp_idx = 0;
    let mut ints_idx = 96;
    for _ in 0..16 {
        let mut l0 = tmp[tmp_idx] << 4;
        l0 |= (tmp[tmp_idx + 1] >> 4) & MASK32_4;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 1] & MASK32_4) << 8;
        l1 |= tmp[tmp_idx + 2];
        ints[ints_idx + 1] = l1;
        tmp_idx += 3;
        ints_idx += 2;
    }
}

fn decode13_to32(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 52, ints, 19, 13, MASK32_13, tmp, MASK32_6);
    let mut tmp_idx = 0;
    let mut ints_idx = 104;
    for _ in 0..4 {
        let mut l0 = tmp[tmp_idx] << 7;
        l0 |= tmp[tmp_idx + 1] << 1;
        l0 |= (tmp[tmp_idx + 2] >> 5) & MASK32_1;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 2] & MASK32_5) << 8;
        l1 |= tmp[tmp_idx + 3] << 2;
        l1 |= (tmp[tmp_idx + 4] >> 4) & MASK32_2;
        ints[ints_idx + 1] = l1;
        let mut l2 = (tmp[tmp_idx + 4] & MASK32_4) << 9;
        l2 |= tmp[tmp_idx + 5] << 3;
        l2 |= (tmp[tmp_idx + 6] >> 3) & MASK32_3;
        ints[ints_idx + 2] = l2;
        let mut l3 = (tmp[tmp_idx + 6] & MASK32_3) << 10;
        l3 |= tmp[tmp_idx + 7] << 4;
        l3 |= (tmp[tmp_idx + 8] >> 2) & MASK32_4;
        ints[ints_idx + 3] = l3;
        let mut l4 = (tmp[tmp_idx + 8] & MASK32_2) << 11;
        l4 |= tmp[tmp_idx + 9] << 5;
        l4 |= (tmp[tmp_idx + 10] >> 1) & MASK32_5;
        ints[ints_idx + 4] = l4;
        let mut l5 = (tmp[tmp_idx + 10] & MASK32_1) << 12;
        l5 |= tmp[tmp_idx + 11] << 6;
        l5 |= tmp[tmp_idx + 12];
        ints[ints_idx + 5] = l5;
        tmp_idx += 13;
        ints_idx += 6;
    }
}

fn decode14_to32(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 56, ints, 18, 14, MASK32_14, tmp, MASK32_4);
    let mut tmp_idx = 0;
    let mut ints_idx = 112;
    for _ in 0..8 {
        let mut l0 = tmp[tmp_idx] << 10;
        l0 |= tmp[tmp_idx + 1] << 6;
        l0 |= tmp[tmp_idx + 2] << 2;
        l0 |= (tmp[tmp_idx + 3] >> 2) & MASK32_2;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 3] & MASK32_2) << 12;
        l1 |= tmp[tmp_idx + 4] << 8;
        l1 |= tmp[tmp_idx + 5] << 4;
        l1 |= tmp[tmp_idx + 6];
        ints[ints_idx + 1] = l1;
        tmp_idx += 7;
        ints_idx += 2;
    }
}

fn decode15_to32(input: &mut Reader<'_>, tmp: &mut [u32; BLOCK_SIZE], ints: &mut [u32; BLOCK_SIZE]) {
    split_ints_tmp(input, 60, ints, 17, 15, MASK32_15, tmp, MASK32_2);
    let mut tmp_idx = 0;
    let mut ints_idx = 120;
    for _ in 0..4 {
        let mut l0 = tmp[tmp_idx] << 13;
        l0 |= tmp[tmp_idx + 1] << 11;
        l0 |= tmp[tmp_idx + 2] << 9;
        l0 |= tmp[tmp_idx + 3] << 7;
        l0 |= tmp[tmp_idx + 4] << 5;
        l0 |= tmp[tmp_idx + 5] << 3;
        l0 |= tmp[tmp_idx + 6] << 1;
        l0 |= (tmp[tmp_idx + 7] >> 1) & MASK32_1;
        ints[ints_idx] = l0;
        let mut l1 = (tmp[tmp_idx + 7] & MASK32_1) << 14;
        l1 |= tmp[tmp_idx + 8] << 12;
        l1 |= tmp[tmp_idx + 9] << 10;
        l1 |= tmp[tmp_idx + 10] << 8;
        l1 |= tmp[tmp_idx + 11] << 6;
        l1 |= tmp[tmp_idx + 12] << 4;
        l1 |= tmp[tmp_idx + 13] << 2;
        l1 |= tmp[tmp_idx + 14];
        ints[ints_idx + 1] = l1;
        tmp_idx += 15;
        ints_idx += 2;
    }
}

fn decode16_to32(input: &mut Reader<'_>, ints: &mut [u32; BLOCK_SIZE]) {
    split_ints(input, 64, ints, 16, 16, MASK32_16, 64, MASK32_16);
}
