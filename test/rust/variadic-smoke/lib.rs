//! C-variadic functions (`...`) for rx-none-elf, called from and calling GCC
//! C code in main.c. rustc reads the arguments with LLVM's `va_arg`
//! (rustc_codegen_llvm va_arg.rs) and leaves their places to the LLVM
//! backend: GCC's rule puts the last named argument and all the unnamed ones
//! on the stack, and `va_list` is a pointer to the next one.

#![no_std]
#![feature(c_variadic_experimental_arch)]

use core::ffi::{
    VaList, c_char, c_double, c_int, c_long, c_longlong, c_uint, c_ulonglong, c_void,
};

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

unsafe extern "C" {
    fn c_sum(n: c_int, ...) -> c_int;
    fn c_mixed(a: c_int, b: c_int, c: c_int, ...) -> c_longlong;
    fn c_dsum(n: c_int, ...) -> c_double;
    fn c_structs(n: c_int, ...) -> c_int;
    fn c_vsum(n: c_int, ap: VaList<'_>) -> c_int;
}

/// What `c_mixed` gets a pointer to.
#[unsafe(no_mangle)]
pub static RS_X: c_int = 0x55;

/// Words written to a buffer that main.c compares.
struct Out {
    p: *mut u32,
    n: usize,
}

impl Out {
    fn put(&mut self, w: u32) {
        unsafe { self.p.add(self.n).write(w) };
        self.n += 1;
    }

    fn put64(&mut self, w: u64) {
        self.put(w as u32);
        self.put((w >> 32) as u32);
    }
}

/// Reads one argument per character of `fmt`, with the type the character
/// names, and writes its bits (the first character for `s`). main.c's
/// `collect` reads the same arguments with GCC's `va_arg`.
unsafe fn collect(out: &mut Out, fmt: *const c_char, ap: &mut VaList<'_>) {
    let mut f = fmt;
    loop {
        let c = unsafe { *f } as u8;
        if c == 0 {
            break;
        }
        unsafe {
            match c {
                b'i' => out.put(ap.next_arg::<c_int>() as u32),
                b'u' => out.put(ap.next_arg::<c_uint>()),
                b'l' => out.put(ap.next_arg::<c_long>() as u32),
                b'q' => out.put64(ap.next_arg::<c_longlong>() as u64),
                b'Q' => out.put64(ap.next_arg::<c_ulonglong>()),
                b'z' => out.put(ap.next_arg::<usize>() as u32),
                b'Z' => out.put(ap.next_arg::<isize>() as u32),
                b'd' => out.put64(ap.next_arg::<c_double>().to_bits()),
                b'p' => out.put(ap.next_arg::<*const c_void>() as usize as u32),
                b's' => out.put(*ap.next_arg::<*const c_char>() as u8 as u32),
                _ => out.put(0xdead_0000 | c as u32),
            }
            f = f.add(1);
        }
    }
}

/// Two named arguments: `out` in R1, `fmt` (the last) on the stack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rs_collect(out: *mut u32, fmt: *const c_char, mut ap: ...) -> c_int {
    let mut o = Out { p: out, n: 0 };
    unsafe { collect(&mut o, fmt, &mut ap) };
    o.n as c_int
}

/// More named arguments than R1-R4 hold: `a` in R1, `b` in R2:R3, `c` in R4,
/// `d` and the rest on the stack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rs_collect_named(
    a: c_int,
    b: c_longlong,
    c: c_int,
    d: c_int,
    out: *mut u32,
    fmt: *const c_char,
    mut ap: ...
) -> c_int {
    let mut o = Out { p: out, n: 0 };
    o.put(a as u32);
    o.put64(b as u64);
    o.put(c as u32);
    o.put(d as u32);
    unsafe { collect(&mut o, fmt, &mut ap) };
    o.n as c_int
}

/// Reads the arguments twice: through a copy (`va_copy`), then the original.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rs_collect_twice(
    out: *mut u32,
    fmt: *const c_char,
    mut ap: ...
) -> c_int {
    let mut o = Out { p: out, n: 0 };
    let mut copy = ap.clone();
    unsafe { collect(&mut o, fmt, &mut copy) };
    drop(copy);
    unsafe { collect(&mut o, fmt, &mut ap) };
    o.n as c_int
}

/// Not variadic: takes the `va_list` of a C function (`c_collect`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rs_vcollect(
    out: *mut u32,
    fmt: *const c_char,
    mut ap: VaList<'_>,
) -> c_int {
    let mut o = Out { p: out, n: 0 };
    unsafe { collect(&mut o, fmt, &mut ap) };
    o.n as c_int
}

/// The sum of `n` ints. One named argument, on the stack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rs_sum(n: c_int, mut ap: ...) -> c_int {
    let mut s: c_int = 0;
    for _ in 0..n {
        s = s.wrapping_add(unsafe { ap.next_arg::<c_int>() });
    }
    s
}

/// Reads the first int, then hands the rest of the list to C's `c_vsum`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rs_fwd(n: c_int, mut ap: ...) -> c_int {
    let first = unsafe { ap.next_arg::<c_int>() };
    first.wrapping_mul(1000).wrapping_add(unsafe { c_vsum(n - 1, ap) })
}

/// Calls GCC's variadic functions, and `rs_sum`, from Rust. main.c makes the
/// same calls from C.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rs_call(out: *mut u32, k: c_int) -> c_int {
    let mut o = Out { p: out, n: 0 };
    unsafe {
        o.put(c_sum(0) as u32);
        o.put(c_sum(1, k) as u32);
        o.put(c_sum(8, 1, 2, 3, 4, 5, 6, 7, k) as u32);
        let x = &RS_X as *const c_int;
        o.put64(c_mixed(1, 2, k, 0x1_2345_6789i64, -5, 2.5f64, x) as u64);
        o.put64(c_dsum(3, 1.5f64, 1.0f64 / 3.0, k as c_double).to_bits());
        o.put(c_structs(2, S8 { a: k, b: -k }, S3 { c: [1, 2, 3] }, k) as u32);
        o.put(c_structs(3, S8 { a: 7, b: k }, S3 { c: [9, 8, k as u8] }, -1) as u32);
        o.put(rs_sum(4, 10, 20, 30, k) as u32);
    }
    o.n as c_int
}
