//! `#![no_std]` Rust code for rx-none-elf, called from (and calling) GCC C
//! code in main.c. Exercises the C ABI (rustc_target callconv/rx.rs) and a
//! good part of `core` (formatting, sorting, 128-bit arithmetic, f64).

#![no_std]

use core::fmt::Write;
use core::sync::atomic::{AtomicU8, AtomicU16, AtomicU32, Ordering};

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

static A32: AtomicU32 = AtomicU32::new(0);
static A16: AtomicU16 = AtomicU16::new(0);
// Four adjacent bytes: the 8-bit atomics work on the containing word and
// must not disturb the neighbours.
static A8: [AtomicU8; 4] = [const { AtomicU8::new(0) }; 4];

/// compare_exchange on A32: 1 if it swapped.
#[unsafe(no_mangle)]
pub extern "C" fn rs_cas32(current: u32, new: u32) -> u32 {
    A32.compare_exchange(current, new, Ordering::AcqRel, Ordering::Acquire)
        .is_ok() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_get32() -> u32 {
    A32.load(Ordering::Relaxed)
}

/// A read-modify-write operation on A32; returns the old value.
#[unsafe(no_mangle)]
pub extern "C" fn rs_rmw32(op: u32, v: u32) -> u32 {
    let o = Ordering::SeqCst;
    match op {
        0 => A32.fetch_add(v, o),
        1 => A32.fetch_sub(v, o),
        2 => A32.fetch_and(v, o),
        3 => A32.fetch_or(v, o),
        4 => A32.fetch_xor(v, o),
        5 => A32.fetch_nand(v, o),
        6 => A32.swap(v, o),
        7 => A32.fetch_max(v, o),
        8 => A32.fetch_min(v, o),
        _ => A32.try_update(o, o, |x| Some(x.wrapping_mul(3) ^ v)).unwrap(),
    }
}

/// fetch_add on byte `i` of A8; returns the old value.
#[unsafe(no_mangle)]
pub extern "C" fn rs_rmw8(i: u32, v: u8) -> u32 {
    A8[i as usize & 3].fetch_add(v, Ordering::SeqCst) as u32
}

/// The four bytes of A8 as a little-endian word.
#[unsafe(no_mangle)]
pub extern "C" fn rs_get8() -> u32 {
    u32::from_le_bytes(A8.each_ref().map(|b| b.load(Ordering::Relaxed)))
}

/// fetch_max on A16 (unsigned) and then a compare_exchange_weak loop that
/// doubles it; returns the old value from fetch_max.
#[unsafe(no_mangle)]
pub extern "C" fn rs_rmw16(v: u16) -> u32 {
    let old = A16.fetch_max(v, Ordering::SeqCst);
    let mut cur = A16.load(Ordering::Relaxed);
    while let Err(x) = A16.compare_exchange_weak(
        cur,
        cur.wrapping_mul(2),
        Ordering::SeqCst,
        Ordering::Relaxed,
    ) {
        cur = x;
    }
    old as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_get16() -> u32 {
    A16.load(Ordering::Relaxed) as u32
}

/// Checked arithmetic that overflows into Option::None.
#[unsafe(no_mangle)]
pub extern "C" fn rs_checked(a: i64, b: i64) -> i64 {
    a.checked_mul(b).unwrap_or(-1)
}
