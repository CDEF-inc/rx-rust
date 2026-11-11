//! A stand-in for `std`, enough to build library/coretests for rx-none-elf
//! (which has no std). It re-exports `core` and `alloc` the way std does,
//! prints to QEMU's SCI0, and has stubs for the few std-only items the tests
//! name: `catch_unwind` (no unwinding: it only calls the closure), panic
//! hooks, `thread::park`, `LazyLock`, `MutexGuard`, `RandomState` and
//! `HashSet` (a `BTreeSet`). Tests that need more are ignored by coretests
//! itself under `panic = "abort"` or listed in scripts/run-coretests.sh.
//! The float methods that only std has (`floor`, `powf`, `gamma`, ...) come
//! from traits in the prelude, implemented as std implements them.

#![no_std]
#![feature(const_trait_impl, core_float_math, core_intrinsics, f16, lang_items)]
#![allow(internal_features)]

extern crate alloc as alloc_crate;
// For its feature names (see there).
extern crate std_features as _;

#[doc(no_inline)]
pub use core::*;

pub use alloc_crate::{format, vec};
pub use core::{
    assert_eq, assert_ne, debug_assert, debug_assert_eq, debug_assert_ne, matches, todo,
    unimplemented, unreachable, write, writeln,
};

pub mod prelude {
    pub mod rust_2024 {
        pub use alloc_crate::borrow::ToOwned;
        pub use alloc_crate::boxed::Box;
        pub use alloc_crate::string::{String, ToString};
        pub use alloc_crate::vec::Vec;
        pub use alloc_crate::{format, vec};
        pub use core::prelude::rust_2024::*;
        // std's prelude has its macros (rustc does not `#[macro_use]` std).
        pub use crate::{eprint, eprintln, print, println, thread_local};
        pub use crate::float::{StdF16 as _, StdF32 as _, StdF32Const as _, StdF64 as _, StdF64Const as _};
    }
    pub use rust_2024 as rust_2021;
    pub use rust_2024 as rust_2018;
    pub use rust_2024 as rust_2015;
    pub use rust_2024 as v1;
}

/// The f16 / f32 / f64 methods of std that core does not have, with std's
/// bodies (library/std/src/num); the C functions are libm's in
/// compiler_builtins.
pub mod float {
    use core::intrinsics;

    unsafe extern "C" {
        safe fn acoshf(x: f32) -> f32;
        safe fn asinhf(x: f32) -> f32;
        safe fn coshf(x: f32) -> f32;
        safe fn lgammaf_r(x: f32, s: &mut i32) -> f32;
        safe fn log1pf(x: f32) -> f32;
        safe fn tgammaf(x: f32) -> f32;
        safe fn acosh(x: f64) -> f64;
        safe fn asinh(x: f64) -> f64;
        safe fn cosh(x: f64) -> f64;
        safe fn lgamma_r(x: f64, s: &mut i32) -> f64;
        safe fn log1p(x: f64) -> f64;
        safe fn tgamma(x: f64) -> f64;
    }

    pub trait StdF16: Sized {
        fn acosh(self) -> Self;
        fn asinh(self) -> Self;
        fn atanh(self) -> Self;
        fn cosh(self) -> Self;
        fn exp(self) -> Self;
        fn exp2(self) -> Self;
        fn gamma(self) -> Self;
        fn ln(self) -> Self;
        fn ln_1p(self) -> Self;
        fn ln_gamma(self) -> (Self, i32);
        fn log(self, base: Self) -> Self;
        fn log10(self) -> Self;
        fn log2(self) -> Self;
        fn powf(self, n: Self) -> Self;
    }

    impl StdF16 for f16 {
        fn acosh(self) -> f16 {
            acoshf(self as f32) as f16
        }
        fn asinh(self) -> f16 {
            asinhf(self as f32) as f16
        }
        fn atanh(self) -> f16 {
            0.5 * ((2.0 * self) / (1.0 - self)).ln_1p()
        }
        fn cosh(self) -> f16 {
            coshf(self as f32) as f16
        }
        fn exp(self) -> f16 {
            intrinsics::exp(self)
        }
        fn exp2(self) -> f16 {
            intrinsics::exp2(self)
        }
        fn gamma(self) -> f16 {
            tgammaf(self as f32) as f16
        }
        fn ln(self) -> f16 {
            intrinsics::log(self)
        }
        fn ln_1p(self) -> f16 {
            log1pf(self as f32) as f16
        }
        fn ln_gamma(self) -> (f16, i32) {
            let mut s = 0;
            let x = lgammaf_r(self as f32, &mut s) as f16;
            (x, s)
        }
        fn log(self, base: f16) -> f16 {
            self.ln() / base.ln()
        }
        fn log10(self) -> f16 {
            intrinsics::log10(self)
        }
        fn log2(self) -> f16 {
            intrinsics::log2(self)
        }
        fn powf(self, n: f16) -> f16 {
            intrinsics::powf16(self, n)
        }
    }

    macro_rules! std_float {
        ($tr:ident, $t:ident, $powf:ident, $acosh:ident, $asinh:ident, $cosh:ident,
         $lgamma_r:ident, $log1p:ident, $tgamma:ident) => {
            pub trait $tr: Sized {
                fn div_euclid(self, rhs: Self) -> Self;
                fn rem_euclid(self, rhs: Self) -> Self;
                fn powi(self, n: i32) -> Self;
                fn powf(self, n: Self) -> Self;
                fn sqrt(self) -> Self;
                fn exp(self) -> Self;
                fn exp2(self) -> Self;
                fn ln(self) -> Self;
                fn log(self, base: Self) -> Self;
                fn log2(self) -> Self;
                fn log10(self) -> Self;
                fn ln_1p(self) -> Self;
                fn cosh(self) -> Self;
                fn asinh(self) -> Self;
                fn acosh(self) -> Self;
                fn atanh(self) -> Self;
                fn gamma(self) -> Self;
                fn ln_gamma(self) -> (Self, i32);
            }

            impl $tr for $t {
                fn div_euclid(self, rhs: $t) -> $t {
                    core::$t::math::div_euclid(self, rhs)
                }
                fn rem_euclid(self, rhs: $t) -> $t {
                    core::$t::math::rem_euclid(self, rhs)
                }
                fn powi(self, n: i32) -> $t {
                    core::$t::math::powi(self, n)
                }
                fn powf(self, n: $t) -> $t {
                    intrinsics::$powf(self, n)
                }
                fn sqrt(self) -> $t {
                    core::$t::math::sqrt(self)
                }
                fn exp(self) -> $t {
                    intrinsics::exp(self)
                }
                fn exp2(self) -> $t {
                    intrinsics::exp2(self)
                }
                fn ln(self) -> $t {
                    intrinsics::log(self)
                }
                fn log(self, base: $t) -> $t {
                    self.ln() / base.ln()
                }
                fn log2(self) -> $t {
                    intrinsics::log2(self)
                }
                fn log10(self) -> $t {
                    intrinsics::log10(self)
                }
                fn ln_1p(self) -> $t {
                    $log1p(self)
                }
                fn cosh(self) -> $t {
                    $cosh(self)
                }
                fn asinh(self) -> $t {
                    $asinh(self)
                }
                fn acosh(self) -> $t {
                    $acosh(self)
                }
                fn atanh(self) -> $t {
                    0.5 * ((2.0 * self) / (1.0 - self)).ln_1p()
                }
                fn gamma(self) -> $t {
                    $tgamma(self)
                }
                fn ln_gamma(self) -> ($t, i32) {
                    let mut s = 0;
                    let x = $lgamma_r(self, &mut s);
                    (x, s)
                }
            }
        };
    }

    std_float!(StdF32, f32, powf32, acoshf, asinhf, coshf, lgammaf_r, log1pf, tgammaf);
    std_float!(StdF64, f64, powf64, acosh, asinh, cosh, lgamma_r, log1p, tgamma);

    // The methods coretests also calls in constants.
    pub const trait StdF32Const: Sized {
        fn floor(self) -> Self;
        fn ceil(self) -> Self;
        fn round(self) -> Self;
        fn round_ties_even(self) -> Self;
        fn trunc(self) -> Self;
        fn fract(self) -> Self;
        fn mul_add(self, a: Self, b: Self) -> Self;
    }

    pub const trait StdF64Const: Sized {
        fn floor(self) -> Self;
        fn ceil(self) -> Self;
        fn round(self) -> Self;
        fn round_ties_even(self) -> Self;
        fn trunc(self) -> Self;
        fn fract(self) -> Self;
        fn mul_add(self, a: Self, b: Self) -> Self;
    }

    const impl StdF32Const for f32 {
        fn floor(self) -> f32 {
            core::f32::math::floor(self)
        }
        fn ceil(self) -> f32 {
            core::f32::math::ceil(self)
        }
        fn round(self) -> f32 {
            core::f32::math::round(self)
        }
        fn round_ties_even(self) -> f32 {
            core::f32::math::round_ties_even(self)
        }
        fn trunc(self) -> f32 {
            core::f32::math::trunc(self)
        }
        fn fract(self) -> f32 {
            core::f32::math::fract(self)
        }
        fn mul_add(self, a: f32, b: f32) -> f32 {
            core::f32::math::mul_add(self, a, b)
        }
    }

    const impl StdF64Const for f64 {
        fn floor(self) -> f64 {
            core::f64::math::floor(self)
        }
        fn ceil(self) -> f64 {
            core::f64::math::ceil(self)
        }
        fn round(self) -> f64 {
            core::f64::math::round(self)
        }
        fn round_ties_even(self) -> f64 {
            core::f64::math::round_ties_even(self)
        }
        fn trunc(self) -> f64 {
            core::f64::math::trunc(self)
        }
        fn fract(self) -> f64 {
            core::f64::math::fract(self)
        }
        fn mul_add(self, a: f64, b: f64) -> f64 {
            core::f64::math::mul_add(self, a, b)
        }
    }
}

pub mod alloc {
    pub use ::alloc_crate::alloc::*;
}
pub mod borrow {
    pub use ::alloc_crate::borrow::*;
}
pub mod boxed {
    pub use ::alloc_crate::boxed::*;
}
pub mod collections {
    pub use ::alloc_crate::collections::*;
    /// Ordered instead of hashed; enough for the tests that collect into one.
    pub type HashSet<T> = ::alloc_crate::collections::BTreeSet<T>;
}
pub mod ffi {
    pub use ::alloc_crate::ffi::{CString, FromVecWithNulError, IntoStringError, NulError};
    pub use core::ffi::*;
}
pub mod fmt {
    pub use ::alloc_crate::fmt::*;
}
pub mod rc {
    pub use ::alloc_crate::rc::*;
}
pub mod slice {
    pub use ::alloc_crate::slice::*;
    pub use core::slice::*;
}
pub mod str {
    pub use ::alloc_crate::str::*;
    pub use core::str::*;
}
pub mod string {
    pub use ::alloc_crate::string::*;
}
pub mod task {
    pub use ::alloc_crate::task::*;
    pub use core::task::*;
}

pub mod hash {
    pub use core::hash::*;

    /// SipHash with fixed keys.
    #[derive(Clone, Debug, Default)]
    #[allow(deprecated)]
    pub struct DefaultHasher(SipHasher);

    impl Hasher for DefaultHasher {
        fn write(&mut self, bytes: &[u8]) {
            self.0.write(bytes)
        }
        fn finish(&self) -> u64 {
            self.0.finish()
        }
    }

    #[derive(Clone, Debug)]
    pub struct RandomState(u64, u64);

    impl RandomState {
        pub fn new() -> Self {
            RandomState(0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210)
        }
    }

    impl Default for RandomState {
        fn default() -> Self {
            Self::new()
        }
    }

    impl BuildHasher for RandomState {
        type Hasher = DefaultHasher;
        #[allow(deprecated)]
        fn build_hasher(&self) -> DefaultHasher {
            DefaultHasher(SipHasher::new_with_keys(self.0, self.1))
        }
    }
}

pub mod panic {
    use ::alloc_crate::boxed::Box;
    use core::any::Any;
    use core::marker::PhantomData;
    pub use core::panic::*;

    /// There is no unwinding: a panic ends the test (see test/), so this
    /// only runs `f`.
    pub fn catch_unwind<F: FnOnce() -> R + UnwindSafe, R>(f: F) -> Result<R, Box<dyn Any + Send>> {
        Ok(f())
    }

    pub struct PanicHookInfo<'a>(PhantomData<&'a ()>);

    type Hook = Box<dyn Fn(&PanicHookInfo<'_>) + Sync + Send + 'static>;

    /// Hooks are not called; the test runner prints panics itself.
    pub fn set_hook(_hook: Hook) {}

    pub fn take_hook() -> Hook {
        Box::new(|_| {})
    }
}

pub mod process {
    /// The `termination` lang item, as in std.
    #[lang = "termination"]
    pub trait Termination {
        fn report(self) -> ExitCode;
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ExitCode(u8);

    impl ExitCode {
        pub const SUCCESS: ExitCode = ExitCode(0);
        pub const FAILURE: ExitCode = ExitCode(1);

        pub fn to_i32(self) -> i32 {
            self.0 as i32
        }
    }

    impl Termination for () {
        fn report(self) -> ExitCode {
            ExitCode::SUCCESS
        }
    }

    impl Termination for ExitCode {
        fn report(self) -> ExitCode {
            self
        }
    }

    impl<T: Termination, E: core::fmt::Debug> Termination for Result<T, E> {
        fn report(self) -> ExitCode {
            match self {
                Ok(v) => v.report(),
                Err(e) => {
                    crate::eprintln!("Error: {e:?}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

pub mod sync {
    pub use ::alloc_crate::sync::*;
    pub use core::sync::*;
    use core::cell::LazyCell;
    use core::ops::Deref;

    /// A `LazyCell` that may be in a static: the tests run on one core
    /// without interrupts.
    pub struct LazyLock<T, F = fn() -> T>(LazyCell<T, F>);

    unsafe impl<T: Sync + Send, F: Send> Sync for LazyLock<T, F> {}

    impl<T, F: FnOnce() -> T> LazyLock<T, F> {
        pub const fn new(f: F) -> Self {
            LazyLock(LazyCell::new(f))
        }
    }

    impl<T, F: FnOnce() -> T> Deref for LazyLock<T, F> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.0
        }
    }

    /// Only named in trait-bound checks (`Sync` when `T: Sync`).
    pub struct MutexGuard<'a, T: ?Sized + 'a>(&'a mut T);
}

pub mod thread {
    use ::alloc_crate::boxed::Box;
    use core::any::Any;

    pub type Result<T> = core::result::Result<T, Box<dyn Any + Send + 'static>>;

    /// What `thread_local!` makes: there is one thread.
    pub struct LocalKey<T: 'static> {
        cell: core::cell::OnceCell<T>,
        init: fn() -> T,
    }

    unsafe impl<T> Sync for LocalKey<T> {}

    impl<T: 'static> LocalKey<T> {
        #[doc(hidden)]
        pub const fn new(init: fn() -> T) -> Self {
            LocalKey { cell: core::cell::OnceCell::new(), init }
        }

        pub fn with<F: FnOnce(&T) -> R, R>(&'static self, f: F) -> R {
            f(self.cell.get_or_init(self.init))
        }
    }

    pub struct Thread;

    impl Thread {
        pub fn unpark(&self) {}
    }

    pub fn current() -> Thread {
        Thread
    }

    /// Returns at once; the caller polls again.
    pub fn park() {}
}

/// On the board (`log-buffer`): the output goes round `coretests_log`, so
/// it keeps the last 192 KB (the end, with the failures and the summary,
/// is what matters; some tests print a lot). `coretests_log_len` counts
/// every byte written; scripts/hw/run.py reads the buffer after the run.
#[cfg(feature = "log-buffer")]
pub mod io {
    use core::fmt::{self, Write};

    const SIZE: usize = 192 * 1024;

    #[unsafe(no_mangle)]
    #[allow(non_upper_case_globals)]
    static mut coretests_log: [u8; SIZE] = [0; SIZE];
    #[unsafe(no_mangle)]
    #[allow(non_upper_case_globals)]
    static mut coretests_log_len: usize = 0;

    struct Log;

    impl Write for Log {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            unsafe {
                let buf = &raw mut coretests_log;
                for &b in s.as_bytes() {
                    (*buf)[coretests_log_len % SIZE] = b;
                    coretests_log_len += 1;
                }
            }
            Ok(())
        }
    }

    #[doc(hidden)]
    pub fn _print(args: fmt::Arguments<'_>) {
        let _ = Log.write_fmt(args);
    }
}

#[cfg(not(feature = "log-buffer"))]
pub mod io {
    use core::fmt::{self, Write};
    use core::ptr::{read_volatile, write_volatile};
    use core::sync::atomic::{AtomicBool, Ordering};

    // SCI0 of QEMU's RX62N.
    const SMR: *mut u8 = 0x0008_8240 as _;
    const BRR: *mut u8 = 0x0008_8241 as _;
    const SCR: *mut u8 = 0x0008_8242 as _;
    const TDR: *mut u8 = 0x0008_8243 as _;
    const SSR: *mut u8 = 0x0008_8244 as _;

    static READY: AtomicBool = AtomicBool::new(false);

    struct Sci0;

    impl Sci0 {
        fn put(&mut self, b: u8) {
            unsafe {
                while read_volatile(SSR) & (1 << 7) == 0 {} // TDRE
                write_volatile(TDR, b);
            }
        }
    }

    impl Write for Sci0 {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            for b in s.bytes() {
                if b == b'\n' {
                    self.put(b'\r');
                }
                self.put(b);
            }
            Ok(())
        }
    }

    #[doc(hidden)]
    pub fn _print(args: fmt::Arguments<'_>) {
        if !READY.swap(true, Ordering::Relaxed) {
            unsafe {
                write_volatile(SCR, 0);
                write_volatile(SMR, 0); // 8N1, PCLK/1
                write_volatile(BRR, 12);
                write_volatile(SCR, 1 << 5); // TE
            }
        }
        let _ = Sci0.write_fmt(args);
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => { $crate::io::_print(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => {{
        $crate::io::_print(format_args!($($arg)*));
        $crate::print!("\n");
    }};
}

#[macro_export]
macro_rules! thread_local {
    () => {};
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = $init:expr $(; $($rest:tt)*)?) => {
        $(#[$attr])*
        $vis static $name: $crate::thread::LocalKey<$t> = $crate::thread::LocalKey::new({
            fn init() -> $t {
                $init
            }
            init
        });
        $($crate::thread_local!($($rest)*);)?
    };
}

#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => { $crate::print!($($arg)*) };
}

#[macro_export]
macro_rules! eprintln {
    ($($arg:tt)*) => { $crate::println!($($arg)*) };
}

/// The `start` lang item: rustc's `main` calls it with the program's `main`.
#[lang = "start"]
fn lang_start<T: process::Termination + 'static>(
    main: fn() -> T,
    _argc: isize,
    _argv: *const *const u8,
    _sigpipe: u8,
) -> isize {
    main().report().to_i32() as isize
}
