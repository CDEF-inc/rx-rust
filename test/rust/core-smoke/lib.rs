//! `#![no_std]` Rust code for rx-none-elf, called from (and calling) GCC C
//! code in main.c. Exercises the C ABI (rustc_target callconv/rx.rs) and a
//! good part of `core` (formatting, sorting, 128-bit arithmetic, f64).

#![no_std]

use core::fmt::Write;
use core::sync::atomic::{AtomicU32, Ordering};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct S3 {
    pub c: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct S8 {
    pub a: i32,
    pub b: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct S20 {
    pub a: [i32; 5],
}

unsafe extern "C" {
    fn c_struct_mix(a: S3, b: S8, c: i32) -> i32;
    fn c_ret20(a: i32) -> S20;
    fn c_double(x: f64) -> f64;
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_add(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_mul64(a: u64, b: u64) -> u64 {
    a.wrapping_mul(b)
}

/// (a * b) / c in 128-bit arithmetic (compiler_builtins' __multi3/__udivti3).
#[unsafe(no_mangle)]
pub extern "C" fn rs_muldiv128(a: u64, b: u64, c: u64) -> u64 {
    ((a as u128 * b as u128) / c as u128) as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_f64(x: f64, y: f32) -> f64 {
    x * 1.5 + y as f64 - 0.25
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_s3(s: S3, y: i32) -> i32 {
    s.c[0] as i32 + s.c[2] as i32 * 256 + y
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_s8(s: S8, k: i32) -> i32 {
    s.a * k - s.b
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_ret20(a: i32) -> S20 {
    S20 { a: [a, a + 1, a + 2, a + 3, a * 10] }
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_ret8(a: i32) -> S8 {
    S8 { a, b: -a }
}

/// Calls back into C with structures passed both ways.
#[unsafe(no_mangle)]
pub extern "C" fn rs_call_c(k: i32) -> i32 {
    let s3 = S3 { c: [k as u8, 7, (k >> 8) as u8] };
    let s8 = S8 { a: k, b: 3 };
    let r = unsafe { c_struct_mix(s3, s8, k) };
    let s20 = unsafe { c_ret20(k) };
    let d = unsafe { c_double(k as f64) };
    r + s20.a[4] + d as i32
}

struct Buf<'a> {
    out: &'a mut [u8],
    len: usize,
}

impl Write for Buf<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let end = self.len + s.len();
        if end > self.out.len() {
            return Err(core::fmt::Error);
        }
        self.out[self.len..end].copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

/// Formats "{a} {b:#x} {c}" into `out`, c = c_hi << 64 | c_lo; returns the
/// length or -1. (GCC has no 128-bit integers on RX.)
#[unsafe(no_mangle)]
pub extern "C" fn rs_format(
    out: *mut u8,
    cap: usize,
    a: i32,
    b: u32,
    c_hi: u64,
    c_lo: u64,
) -> i32 {
    let c = (c_hi as u128) << 64 | c_lo as u128;
    let out = unsafe { core::slice::from_raw_parts_mut(out, cap) };
    let mut buf = Buf { out, len: 0 };
    match write!(buf, "{a} {b:#x} {c}") {
        Ok(()) => buf.len as i32,
        Err(_) => -1,
    }
}

/// Sorts `n` values in place and returns their checksum.
#[unsafe(no_mangle)]
pub extern "C" fn rs_sort(p: *mut i32, n: usize) -> u32 {
    let v = unsafe { core::slice::from_raw_parts_mut(p, n) };
    v.sort_unstable();
    v.iter().enumerate().fold(0u32, |acc, (i, &x)| {
        acc.wrapping_mul(31).wrapping_add(x as u32 ^ i as u32)
    })
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn rs_atomic(v: u32) -> u32 {
    let old = COUNTER.load(Ordering::Acquire);
    COUNTER.store(old.wrapping_add(v), Ordering::Release);
    COUNTER.load(Ordering::Relaxed)
}

/// Checked arithmetic that overflows into Option::None.
#[unsafe(no_mangle)]
pub extern "C" fn rs_checked(a: i64, b: i64) -> i64 {
    a.checked_mul(b).unwrap_or(-1)
}
