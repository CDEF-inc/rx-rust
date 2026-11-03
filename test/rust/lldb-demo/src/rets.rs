//! Functions that return values of the shapes Rust's ABI distinguishes on
//! RX, for `finish` in scripts/check-lldb.sh: a pair of scalars comes back
//! as two values (R1, R2), other aggregates of up to 8 bytes as an integer
//! in R1(:R2), larger ones in memory (which LLDB cannot show).

use core::hint::black_box;

#[derive(Clone, Copy)]
pub struct S2 {
    pub a: u8,
    pub b: u8,
}

#[derive(Clone, Copy)]
pub struct S3 {
    pub a: u8,
    pub b: u8,
    pub c: u8,
}

#[derive(Clone, Copy)]
pub struct S8 {
    pub a: u32,
    pub b: i32,
}

#[derive(Clone, Copy)]
pub struct S12 {
    pub a: u32,
    pub b: u32,
    pub c: u32,
}

#[derive(Clone, Copy)]
pub struct S16 {
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub d: u32,
}

#[derive(Clone, Copy)]
pub struct S20 {
    pub a: [u32; 5],
}

#[inline(never)]
pub fn ret_s2(x: u8) -> S2 {
    S2 { a: x, b: x + 1 }
}

#[inline(never)]
pub fn ret_s3(x: u8) -> S3 {
    S3 { a: x, b: x + 1, c: x + 2 }
}

#[inline(never)]
pub fn ret_pair16(x: u16) -> (u16, u16) {
    (x, 0x1234)
}

#[inline(never)]
pub fn ret_s8(x: u32) -> S8 {
    S8 { a: x, b: -1 }
}

#[inline(never)]
pub fn ret_s12(x: u32) -> S12 {
    S12 { a: x, b: 2, c: 3 }
}

#[inline(never)]
pub fn ret_s16(x: u32) -> S16 {
    S16 { a: x, b: 2, c: 3, d: 4 }
}

#[inline(never)]
pub fn ret_s20(x: u32) -> S20 {
    S20 { a: [x, 2, 3, 4, 5] }
}

#[inline(never)]
pub fn ret_option(x: u32) -> Option<u32> {
    if x > 0 { Some(x) } else { None }
}

#[inline(never)]
pub fn ret_u64(x: u32) -> u64 {
    (x as u64) << 33 | 5
}

#[inline(never)]
pub fn ret_f64(x: u32) -> f64 {
    x as f64 + 0.5
}

/// Calls them all; the results go through black_box so that they are kept.
#[inline(never)]
pub fn run() {
    black_box(ret_s2(black_box(b'A')));
    black_box(ret_s3(black_box(b'a')));
    black_box(ret_pair16(black_box(16)));
    black_box(ret_s8(black_box(8)));
    black_box(ret_s12(black_box(12)));
    black_box(ret_s16(black_box(16)));
    black_box(ret_s20(black_box(20)));
    black_box(ret_option(black_box(42)));
    black_box(ret_u64(black_box(1)));
    black_box(ret_f64(black_box(3)));
}
