//! char / short / bool arguments of `extern "C"` functions between Rust and
//! GCC (main.c). GCC does not extend such arguments: the caller leaves the
//! upper bits of the register undefined (`add r2, r1` for the sum of two
//! `unsigned char`s, `mov.b [r1], r1` that sign-extends an `unsigned char`,
//! an `unsigned` passed on as it is), and the callee extends the value
//! itself. rustc's RX C ABI (rustc_target callconv/rx.rs) must not mark them
//! `zeroext` / `signext`, or the Rust callee trusts the upper bits. Return
//! values are extended by the callee, in GCC and in Rust.

#![no_std]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct S3 {
    pub c: [u8; 3],
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_u8(a: u8) -> u32 {
    a as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_i8(a: i8) -> i32 {
    a as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_u16(a: u16) -> u32 {
    a as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_i16(a: i16) -> i32 {
    a as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_bool(b: bool) -> u32 {
    if b { 0x1234 } else { 0x5678 }
}

/// R1-R4.
#[unsafe(no_mangle)]
pub extern "C" fn rs_mix4(a: u8, b: i8, c: u16, d: i16) -> i32 {
    a as i32 + (b as i32) * 1000 + (c as i32) * 3 - (d as i32) * 7
}

/// `a` in R1:R2, `b` in R3, `c` in R4.
#[unsafe(no_mangle)]
pub extern "C" fn rs_after_i64(a: i64, b: u8, c: i16) -> i32 {
    (a as i32) ^ ((b as i32) << 8) ^ ((c as i32) << 16)
}

/// `b` in R1, `s` on the stack (3 bytes). After `s` the two conventions
/// differ (CC-RX would put a `u8` in R1, GNURX in R2), so it comes first.
#[unsafe(no_mangle)]
pub extern "C" fn rs_before_s3(b: u8, s: S3) -> u32 {
    s.c[0] as u32 + (s.c[1] as u32) * 10 + (s.c[2] as u32) * 100 + (b as u32) * 1000
}

/// Truncated to 8 / 16 bits; the caller (GCC) relies on the extension.
#[unsafe(no_mangle)]
pub extern "C" fn rs_ret_u8(x: u32) -> u8 {
    x as u8
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_ret_i8(x: u32) -> i8 {
    x as i8
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_ret_u16(x: u32) -> u16 {
    x as u16
}

#[unsafe(no_mangle)]
pub extern "C" fn rs_ret_i16(x: u32) -> i16 {
    x as i16
}

unsafe extern "C" {
    fn c_u8(a: u8) -> u32;
    fn c_i8(a: i8) -> i32;
    fn c_u16(a: u16) -> u32;
    fn c_i16(a: i16) -> i32;
    fn c_bool(b: bool) -> u32;
    fn c_mix4(a: u8, b: i8, c: u16, d: i16) -> i32;
    fn c_ret_u8(x: u32) -> u8;
    fn c_ret_i16(x: u32) -> i16;
}

/// Calls GCC's functions from Rust with values computed from `k`, which
/// LLVM may leave untruncated in the register. main.c compares the words.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rs_call(out: *mut u32, k: u32) -> i32 {
    let mut n = 0;
    let mut put = |w: u32| {
        unsafe { out.add(n).write(w) };
        n += 1;
    };
    unsafe {
        put(c_u8((k as u8).wrapping_mul(100)));
        put(c_i8((k as i8).wrapping_mul(50)) as u32);
        put(c_u16((k as u16).wrapping_mul(30000)));
        put(c_i16((k as i16).wrapping_mul(-20000)) as u32);
        put(c_bool(k & 2 != 0));
        put(c_bool(k & 4 != 0));
        let (a, b) = ((k as u8).wrapping_add(254), (k as i8).wrapping_mul(43));
        put(c_mix4(a, b, (k as u16) << 14, (k as i16) << 13) as u32);
        put(c_ret_u8(k + 0x1fc) as u32);
        put(c_ret_i16(k + 0x17ffc) as i32 as u32);
    }
    n as i32
}
