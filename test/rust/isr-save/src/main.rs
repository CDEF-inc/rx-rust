//! Checks that rx-rt's interrupt entry preserves FPSW and the accumulator
//! (`save-acc`), and that handlers start with FPSW = 0x100. Built for RXv1
//! with the FPU (QEMU's RX62N); run with scripts/run-rust-rom.sh
//! test/rust/isr-save. The results are left in `checks` / `failures` /
//! `finished` for the QEMU monitor.

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]
// The result variables keep the names the QEMU scripts read.
#![allow(non_upper_case_globals)]

use core::arch::asm;
use core::panic::PanicInfo;
use core::ptr::write_volatile;
use core::sync::atomic::{AtomicU32, Ordering};

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

static CALLS: AtomicU32 = AtomicU32::new(0);
static ENTRY_FPSW: AtomicU32 = AtomicU32::new(0);
static CONVERTED: AtomicU32 = AtomicU32::new(0);

fn fpsw() -> u32 {
    let v: u32;
    unsafe { asm!("mvfc fpsw, {}", out(reg) v, options(nomem, nostack)) };
    v
}

// Bits 63-32 and 47-16 of the (RXv1) accumulator.
fn acc() -> (u32, u32) {
    let (hi, mi): (u32, u32);
    unsafe {
        asm!("mvfachi {0}", "mvfacmi {1}", out(reg) hi, out(reg) mi,
             options(nomem, nostack))
    };
    (hi, mi)
}

fn set_acc(hi: u32, lo: u32) {
    unsafe {
        asm!("mvtachi {0}", "mvtaclo {1}", in(reg) hi, in(reg) lo,
             options(nomem, nostack))
    };
}

// Changes FPSW (rounding towards -infinity), converts with the FPU (itof)
// and overwrites the accumulator.
extern "C" fn soft() {
    ENTRY_FPSW.store(fpsw(), Ordering::Relaxed);
    unsafe { asm!("mvtc #3, fpsw", options(nomem, nostack)) };
    let n = CALLS.load(Ordering::Relaxed);
    CONVERTED.store((core::hint::black_box(n as i32) + 3) as f32 as u32,
                    Ordering::Relaxed);
    set_acc(0xdead_beef, 0x0bad_f00d);
    CALLS.store(n + 1, Ordering::Relaxed);
}
rx_rt::interrupt!(65, soft);

#[unsafe(no_mangle)]
fn main() -> ! {
    // rx-rt sets FPSW to 0x100 at reset.
    check(1, fpsw() == 0x100);

    for round in 0..3u32 {
        // Rounding towards zero in the interrupted code. (The cause flags
        // cannot be set with MVTC on QEMU, so they are not used.)
        unsafe { asm!("mvtc #0x101, fpsw", options(nomem, nostack)) };
        let hi = 0x1234_5678 ^ round;
        set_acc(hi, 0x9abc_0000);
        let id = 10 * (round + 1);
        check(id, fpsw() == 0x101);
        unsafe { asm!("int #65") };
        check(id + 1, CALLS.load(Ordering::Relaxed) == round + 1);
        check(id + 2, fpsw() == 0x101);
        check(id + 3, acc() == (hi, 0x5678_9abc ^ (round << 16)));
        check(id + 4, ENTRY_FPSW.load(Ordering::Relaxed) == 0x100);
        check(id + 5, CONVERTED.load(Ordering::Relaxed) == round + 3);
    }

    unsafe { write_volatile(&raw mut finished, 1) };
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
