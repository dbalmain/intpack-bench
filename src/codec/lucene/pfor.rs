//! Lucene 10.3.1 `PForUtil`: patched frame-of-reference over a 128-int block.
//!
//! Token byte is `(num_exceptions << 5) | bpv`. The all-equal special case
//! writes bpv 0 and a vint of the common (unpatched) value. Exceptions are
//! `[index u8][high-byte]` pairs. Java's heap compares are signed; this port
//! uses unsigned comparison, which agrees on the non-negative values Lucene
//! stores and is the natural extension past `2^31`.
#![cfg_attr(not(test), allow(dead_code))] // next slice's codec adapter

use super::for_util::{self, BLOCK_SIZE, bits_required, num_bytes};
use super::io::{Reader, write_vint};

const MAX_EXCEPTIONS: usize = 7;

pub fn encode(ints: &[u32; BLOCK_SIZE], out: &mut Vec<u8>) {
    let mut values = *ints;

    let mut heap = [0u32; MAX_EXCEPTIONS + 1];
    heap.copy_from_slice(&values[..MAX_EXCEPTIONS + 1]);
    min_heapify(&mut heap);
    for &v in &values[MAX_EXCEPTIONS + 1..] {
        if v > heap[0] {
            heap[0] = v;
            sift_down(&mut heap, 0);
        }
    }
    let top_value = heap[0];
    let mut max = 0u32;
    for &h in &heap {
        if h > max {
            max = h;
        }
    }

    let max_bits_required = bits_required(max);
    let patched_bits_required = bits_required(top_value).max(max_bits_required.saturating_sub(8));
    let max_unpatched = if patched_bits_required >= 32 { u32::MAX } else { (1u32 << patched_bits_required) - 1 };

    let mut exceptions = Vec::new();
    for (i, v) in values.iter_mut().enumerate() {
        if *v > max_unpatched {
            exceptions.push(i as u8);
            exceptions.push((*v >> patched_bits_required) as u8);
            *v &= max_unpatched;
        }
    }
    let num_exceptions = exceptions.len() / 2;

    if all_equal(&values) && max_bits_required <= 8 {
        for chunk in exceptions.chunks_exact_mut(2) {
            chunk[1] = (u32::from(chunk[1]) << patched_bits_required) as u8;
        }
        out.push((num_exceptions as u8) << 5);
        write_vint(out, values[0]);
    } else {
        let token = ((num_exceptions as u8) << 5) | (patched_bits_required as u8);
        out.push(token);
        for_util::encode(&values, patched_bits_required, out);
    }
    out.extend_from_slice(&exceptions);
}

pub fn decode(input: &[u8], out: &mut [u32; BLOCK_SIZE]) -> usize {
    let mut r = Reader::new(input);
    let token = r.read_byte();
    let bpv = u32::from(token & 0x1f);
    let num_exceptions = usize::from(token >> 5);
    if bpv == 0 {
        let v = r.read_vint();
        *out = [v; BLOCK_SIZE];
    } else {
        let rest = r.buf.get(r.pos..).unwrap_or(&[]);
        for_util::decode(bpv, rest, out);
        r.pos += num_bytes(bpv);
    }
    for _ in 0..num_exceptions {
        let idx = usize::from(r.read_byte());
        let hi = u32::from(r.read_byte());
        if let Some(slot) = out.get_mut(idx) {
            *slot |= hi << bpv;
        }
    }
    r.pos
}

pub fn skip(input: &[u8]) -> usize {
    let mut r = Reader::new(input);
    let token = r.read_byte();
    let bpv = u32::from(token & 0x1f);
    let num_exceptions = usize::from(token >> 5);
    if bpv == 0 {
        let _ = r.read_vlong();
        r.pos + num_exceptions * 2
    } else {
        r.pos + num_bytes(bpv) + num_exceptions * 2
    }
}

fn all_equal(ints: &[u32; BLOCK_SIZE]) -> bool {
    ints.iter().all(|&v| v == ints[0])
}

fn min_heapify(heap: &mut [u32]) {
    for i in (0..heap.len() / 2).rev() {
        sift_down(heap, i);
    }
}

fn sift_down(heap: &mut [u32], mut i: usize) {
    let len = heap.len();
    loop {
        let left = 2 * i + 1;
        let right = left + 1;
        let mut smallest = i;
        if left < len && heap[left] < heap[smallest] {
            smallest = left;
        }
        if right < len && heap[right] < heap[smallest] {
            smallest = right;
        }
        if smallest == i {
            break;
        }
        heap.swap(i, smallest);
        i = smallest;
    }
}
