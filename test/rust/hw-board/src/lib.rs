//! What the board tests (test/rust/hw-*) share: the board, picked by a
//! feature (exactly one), its HAL as [`hal`] and memory.x (build.rs), and
//! the result variables scripts/hw/run.py reads.
//!
//! - `ap-rx72m`: the AP-RX72M-0A, rx72-hal for the RX72M.
//! - `ek-rx74n`: the EK-RX74N, rx74-hal for the RX74N.
//! - `ek-rx74m`: the EK-RX74M, rx74-hal for the RX74M.
//!
//! A test calls [`check`] for each check and [`finish`] at the end; a panic
//! counts as failure 999. The runner stops at `_done` and reads `checks`,
//! `failures`, `first_failure` and `finished`.

#![no_std]
// The result variables keep the names the test scripts read.
#![allow(non_upper_case_globals)]

use core::panic::PanicInfo;
use core::ptr::write_volatile;

#[cfg(not(any(feature = "ap-rx72m", feature = "ek-rx74n", feature = "ek-rx74m")))]
compile_error!("pick the board: --features ap-rx72m, ek-rx74n or ek-rx74m");
#[cfg(any(
    all(feature = "ap-rx72m", any(feature = "ek-rx74n", feature = "ek-rx74m")),
    all(feature = "ek-rx74n", feature = "ek-rx74m")
))]
compile_error!("pick one board only (ap-rx72m, ek-rx74n or ek-rx74m)");

#[cfg(feature = "ap-rx72m")]
pub use rx72_hal as hal;
#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
pub use rx74_hal as hal;

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut first_failure: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;

/// Counts a check, and a failure (the first one's `id` in `first_failure`)
/// unless `ok`.
pub fn check(id: u32, ok: bool) {
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

/// Sets `finished` and ends the test at `_done`.
pub fn finish() -> ! {
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
