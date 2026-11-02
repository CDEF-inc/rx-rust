//! Ordinary Rust `f32` / `f64` arithmetic, compares and `as` conversions,
//! checked against compiler-builtins' IEEE soft-float functions called
//! directly. Built with `-C target-feature=+flush-denormals` the operations
//! use the FPU (and the DFPU on RXv3); without it they are soft-float. NaNs,
//! infinities and signed zeros must give the IEEE results either way (a NaN
//! result only has to be a NaN); cases with a denormal result are skipped.
//! A small benchmark (dot products and a polynomial) is checked the same
//! way. Built for QEMU's rx62n by default; scripts/run-rust-rom.sh
//! test/rust/float-smoke, with RX_CPU and RUST_EXTRA_FLAGS for the others.
//! The results are left in `checks` / `failures` / `finished`.

#![no_std]
#![no_main]
// The result variables keep the names the QEMU scripts read.
#![allow(non_upper_case_globals)]

use core::cmp::Ordering;
use core::hint::black_box;
use core::panic::PanicInfo;
use core::ptr::write_volatile;

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut first_failure: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;

fn check(id: u32, ok: bool) {
    unsafe {
        checks += 1;
        if !ok {
            if failures == 0 {
                first_failure = id;
            }
            failures += 1;
        }
    }
}

// compiler-builtins (IEEE 754 in software).
unsafe extern "C" {
    fn __addsf3(a: f32, b: f32) -> f32;
    fn __subsf3(a: f32, b: f32) -> f32;
    fn __mulsf3(a: f32, b: f32) -> f32;
    fn __divsf3(a: f32, b: f32) -> f32;
    fn __adddf3(a: f64, b: f64) -> f64;
    fn __subdf3(a: f64, b: f64) -> f64;
    fn __muldf3(a: f64, b: f64) -> f64;
    fn __divdf3(a: f64, b: f64) -> f64;
    fn __unordsf2(a: f32, b: f32) -> i32;
    fn __eqsf2(a: f32, b: f32) -> i32;
    fn __ltsf2(a: f32, b: f32) -> i32;
    fn __lesf2(a: f32, b: f32) -> i32;
    fn __gtsf2(a: f32, b: f32) -> i32;
    fn __gesf2(a: f32, b: f32) -> i32;
    fn __unorddf2(a: f64, b: f64) -> i32;
    fn __eqdf2(a: f64, b: f64) -> i32;
    fn __ltdf2(a: f64, b: f64) -> i32;
    fn __ledf2(a: f64, b: f64) -> i32;
    fn __gtdf2(a: f64, b: f64) -> i32;
    fn __gedf2(a: f64, b: f64) -> i32;
    fn __fixsfsi(a: f32) -> i32;
    fn __fixunssfsi(a: f32) -> u32;
    fn __fixdfsi(a: f64) -> i32;
    fn __fixunsdfsi(a: f64) -> u32;
    fn __extendsfdf2(a: f32) -> f64;
    fn __truncdfsf2(a: f64) -> f32;
}

// NaN tests on the bits: `is_nan` is a float compare (x != x), which is
// one of the operations under test.
fn f_nan(x: f32) -> bool {
    x.to_bits() & 0x7fff_ffff > 0x7f80_0000
}
fn d_nan(x: f64) -> bool {
    x.to_bits() & 0x7fff_ffff_ffff_ffff > 0x7ff0_0000_0000_0000
}

fn f_denormal(b: u32) -> bool {
    b & 0x7f80_0000 == 0 && b & 0x007f_ffff != 0
}
fn d_denormal(b: u64) -> bool {
    b & 0x7ff0_0000_0000_0000 == 0 && b & 0x000f_ffff_ffff_ffff != 0
}

fn check_f(id: u32, got: f32, want: f32) {
    if f_denormal(want.to_bits()) {
        return;
    }
    check(id, if f_nan(want) { f_nan(got) } else { got.to_bits() == want.to_bits() });
}
fn check_d(id: u32, got: f64, want: f64) {
    if d_denormal(want.to_bits()) {
        return;
    }
    check(id, if d_nan(want) { d_nan(got) } else { got.to_bits() == want.to_bits() });
}

// The operations under test (not inlined into the checks).
#[inline(never)]
fn f_ops(a: f32, b: f32) -> [f32; 4] {
    [a + b, a - b, a * b, a / b]
}
#[inline(never)]
fn d_ops(a: f64, b: f64) -> [f64; 4] {
    [a + b, a - b, a * b, a / b]
}
#[inline(never)]
fn f_cmps(a: f32, b: f32) -> [bool; 6] {
    [a == b, a != b, a < b, a <= b, a > b, a >= b]
}
#[inline(never)]
fn d_cmps(a: f64, b: f64) -> [bool; 6] {
    [a == b, a != b, a < b, a <= b, a > b, a >= b]
}
#[inline(never)]
fn f_partial(a: f32, b: f32) -> Option<Ordering> {
    a.partial_cmp(&b)
}
#[inline(never)]
fn d_partial(a: f64, b: f64) -> Option<Ordering> {
    a.partial_cmp(&b)
}
#[inline(never)]
fn f_as(a: f32) -> (i32, u32, f64) {
    (a as i32, a as u32, a as f64)
}
#[inline(never)]
fn d_as(a: f64) -> (i32, u32, f32) {
    (a as i32, a as u32, a as f32)
}

// The ordered answers from compiler-builtins (false when unordered).
struct Ref {
    un: bool,
    eq: bool,
    lt: bool,
    le: bool,
    gt: bool,
    ge: bool,
}

fn f_ref(a: f32, b: f32) -> Ref {
    unsafe {
        let un = __unordsf2(a, b) != 0;
        Ref {
            un,
            eq: !un && __eqsf2(a, b) == 0,
            lt: __ltsf2(a, b) < 0,
            le: __lesf2(a, b) <= 0,
            gt: __gtsf2(a, b) > 0,
            ge: __gesf2(a, b) >= 0,
        }
    }
}
fn d_ref(a: f64, b: f64) -> Ref {
    unsafe {
        let un = __unorddf2(a, b) != 0;
        Ref {
            un,
            eq: !un && __eqdf2(a, b) == 0,
            lt: __ltdf2(a, b) < 0,
            le: __ledf2(a, b) <= 0,
            gt: __gtdf2(a, b) > 0,
            ge: __gedf2(a, b) >= 0,
        }
    }
}

fn check_cmps(id: u32, got: [bool; 6], partial: Option<Ordering>, r: Ref) {
    check(id, got == [r.eq, !r.eq, r.lt, r.le, r.gt, r.ge]);
    let want = if r.un {
        None
    } else if r.lt {
        Some(Ordering::Less)
    } else if r.gt {
        Some(Ordering::Greater)
    } else {
        Some(Ordering::Equal)
    };
    check(id + 1, partial == want);
}

const FV: [u32; 18] = [
    0x0000_0000, 0x8000_0000, 0x3f80_0000, 0xbf80_0000, 0x3fc0_0000, 0xc010_0000,
    0x7f7f_ffff, 0xff7f_ffff, 0x7e96_7699, 0x1e3c_e508, 0x9e3c_e508, // 1e38, ±1e-20
    0x7f80_0000, 0xff80_0000, 0x7fc0_0000, 0xffc0_0000, 0x7fc1_2345, 0x7fff_ffff,
    0x7fa0_0000, // a signalling NaN
];
const DV: [u64; 18] = [
    0x0000_0000_0000_0000, 0x8000_0000_0000_0000, 0x3ff0_0000_0000_0000,
    0xbff0_0000_0000_0000, 0x3ff8_0000_0000_0000, 0xc002_0000_0000_0000,
    0x7fef_ffff_ffff_ffff, 0xffef_ffff_ffff_ffff, 0x7e37_e43c_8800_759c,
    0x1a56_e1fc_2f8f_359d, 0x9a56_e1fc_2f8f_359d, // 1e300, ±1e-180
    0x7ff0_0000_0000_0000, 0xfff0_0000_0000_0000, 0x7ff8_0000_0000_0000,
    0xfff8_0000_0000_0000, 0x7ff8_0000_1234_5678, 0x7fff_ffff_ffff_ffff,
    0x7ff4_0000_0000_0000, // a signalling NaN
];
// Conversions: values around the 32-bit limits.
const FC: [u32; 17] = [
    0x0000_0000, 0x8000_0000, 0x3f40_0000, 0xbf40_0000, 0x42f7_cccd, 0xc2f7_cccd,
    0x4eff_ffff, 0x4f00_0000, 0xcf00_0000, 0xcf00_0001, 0x4f7f_ffff, 0x4f80_0000,
    0xbf80_0000, 0x7f80_0000, 0xff80_0000, 0x7fc0_0000, 0xffc0_0000,
];
const DC: [u64; 17] = [
    0x0000_0000_0000_0000, 0x8000_0000_0000_0000, 0x3fe8_0000_0000_0000,
    0xbfe8_0000_0000_0000, 0x41df_ffff_ffc0_0000, 0x41df_ffff_ffe0_0000,
    0x41e0_0000_0000_0000, 0xc1e0_0000_0000_0000, 0xc1e0_0000_0020_0000,
    0x41ef_ffff_ffe0_0000, 0x41ef_ffff_ffff_f000, 0x41f0_0000_0000_0000,
    0xbff0_0000_0000_0000, 0x7ff0_0000_0000_0000, 0xfff0_0000_0000_0000,
    0x7ff8_0000_0000_0000, 0xfff8_0000_0000_0000,
];

fn arithmetic() {
    for &x in &FV {
        for &y in &FV {
            let (a, b) = (black_box(f32::from_bits(x)), black_box(f32::from_bits(y)));
            let got = f_ops(a, b);
            unsafe {
                check_f(1, got[0], __addsf3(a, b));
                check_f(2, got[1], __subsf3(a, b));
                check_f(3, got[2], __mulsf3(a, b));
                check_f(4, got[3], __divsf3(a, b));
            }
            check_cmps(5, f_cmps(a, b), f_partial(a, b), f_ref(a, b));
        }
    }
    for &x in &DV {
        for &y in &DV {
            let (a, b) = (black_box(f64::from_bits(x)), black_box(f64::from_bits(y)));
            let got = d_ops(a, b);
            unsafe {
                check_d(11, got[0], __adddf3(a, b));
                check_d(12, got[1], __subdf3(a, b));
                check_d(13, got[2], __muldf3(a, b));
                check_d(14, got[3], __divdf3(a, b));
            }
            check_cmps(15, d_cmps(a, b), d_partial(a, b), d_ref(a, b));
        }
    }
}

fn conversions() {
    for &x in &FC {
        let a = black_box(f32::from_bits(x));
        let (i, u, d) = f_as(a);
        let r = |b: f32| f_ref(a, b);
        // `as` saturates and gives 0 for a NaN.
        let want_i = if f_nan(a) {
            0
        } else if r(-2147483648.0).le {
            i32::MIN
        } else if r(2147483648.0).ge {
            i32::MAX
        } else {
            unsafe { __fixsfsi(a) }
        };
        let want_u = if f_nan(a) || r(0.0).le {
            0
        } else if r(4294967296.0).ge {
            u32::MAX
        } else {
            unsafe { __fixunssfsi(a) }
        };
        check(21, i == want_i);
        check(22, u == want_u);
        check_d(23, d, unsafe { __extendsfdf2(a) });
    }
    for &x in &DC {
        let a = black_box(f64::from_bits(x));
        let (i, u, f) = d_as(a);
        let r = |b: f64| d_ref(a, b);
        let want_i = if d_nan(a) {
            0
        } else if r(-2147483648.0).le {
            i32::MIN
        } else if r(2147483647.0).ge {
            i32::MAX
        } else {
            unsafe { __fixdfsi(a) }
        };
        let want_u = if d_nan(a) || r(0.0).le {
            0
        } else if r(4294967295.0).ge {
            u32::MAX
        } else {
            unsafe { __fixunsdfsi(a) }
        };
        check(24, i == want_i);
        check(25, u == want_u);
        check_f(26, f, unsafe { __truncdfsf2(a) });
    }
    for &x in &DV {
        let a = black_box(f64::from_bits(x));
        check_f(27, d_as(a).2, unsafe { __truncdfsf2(a) });
    }
}

// The benchmark: dot products and a Horner polynomial over 64 values,
// checked against the same computation in soft-float.
const N: usize = 64;

#[inline(never)]
fn bench_f(x: &[f32; N], y: &[f32; N]) -> f32 {
    let mut acc = 0.0f32;
    for i in 0..N {
        acc += x[i] * y[i];
        let t = x[i];
        acc += ((0.5 * t - 1.25) * t + 2.0) * t - 0.75;
    }
    acc
}
fn bench_f_ref(x: &[f32; N], y: &[f32; N]) -> f32 {
    let mut acc = 0.0f32;
    unsafe {
        for i in 0..N {
            acc = __addsf3(acc, __mulsf3(x[i], y[i]));
            let t = x[i];
            let p = __subsf3(
                __mulsf3(__addsf3(__mulsf3(__subsf3(__mulsf3(0.5, t), 1.25), t), 2.0), t),
                0.75,
            );
            acc = __addsf3(acc, p);
        }
    }
    acc
}
#[inline(never)]
fn bench_d(x: &[f64; N], y: &[f64; N]) -> f64 {
    let mut acc = 0.0f64;
    for i in 0..N {
        acc += x[i] * y[i];
        let t = x[i];
        acc += ((0.5 * t - 1.25) * t + 2.0) * t - 0.75;
    }
    acc
}
fn bench_d_ref(x: &[f64; N], y: &[f64; N]) -> f64 {
    let mut acc = 0.0f64;
    unsafe {
        for i in 0..N {
            acc = __adddf3(acc, __muldf3(x[i], y[i]));
            let t = x[i];
            let p = __subdf3(
                __muldf3(__adddf3(__muldf3(__subdf3(__muldf3(0.5, t), 1.25), t), 2.0), t),
                0.75,
            );
            acc = __adddf3(acc, p);
        }
    }
    acc
}

fn bench() {
    let mut xf = [0.0f32; N];
    let mut yf = [0.0f32; N];
    let mut xd = [0.0f64; N];
    let mut yd = [0.0f64; N];
    for i in 0..N {
        // i / 8 - 3 and (i * 3 % 17) / 4 + 0.5: exact in both types
        xf[i] = (i as i32 - 24) as f32 * 0.125;
        yf[i] = ((i * 3 % 17) as i32) as f32 * 0.25 + 0.5;
        xd[i] = xf[i] as f64;
        yd[i] = yf[i] as f64;
    }
    let mut sf = 0.0f32;
    let mut sd = 0.0f64;
    // 10 rounds: soft-float takes about 0.8 s a round on the RX72M as rx-rt
    // leaves it (LOCO clock), the FPU / DFPU version much less.
    for _ in 0..10 {
        sf = bench_f(black_box(&xf), black_box(&yf));
        sd = bench_d(black_box(&xd), black_box(&yd));
    }
    check_f(31, sf, bench_f_ref(&xf, &yf));
    check_d(32, sd, bench_d_ref(&xd, &yd));
}

#[unsafe(no_mangle)]
fn main() -> ! {
    arithmetic();
    conversions();
    bench();
    unsafe { write_volatile(&raw mut finished, 1) };
    done()
}

// The end of the test (the board runner's breakpoint).
#[unsafe(no_mangle)]
#[inline(never)]
extern "C" fn done() -> ! {
    loop {
        rx_rt::interrupt::wait();
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe {
        failures += 1;
        first_failure = 999;
    }
    loop {}
}
