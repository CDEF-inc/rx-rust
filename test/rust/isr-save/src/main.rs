//! Checks that rx-rt's interrupt entry preserves FPSW and the accumulator
//! (`save-acc`), and that handlers start with FPSW = 0x100. On RXv2 and
//! later it also checks A1 with its guard bits, and with the DFPU DR0 / DR1,
//! DPSW and DCMR (and that handlers start with DPSW = 0x100). On RXv3e it
//! runs `int Rs`, the 5-bit `mvtipl` and the new control registers from
//! `asm!`, which needs rustc's LLVM to know them. Built for
//! QEMU's rx62n (RXv1 with the FPU) by default; scripts/run-rust-rom.sh
//! test/rust/isr-save, with RX_CPU for the other cores. The results are
//! left in `checks` / `failures` / `finished` for the QEMU monitor.

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
static ENTRY_DPSW: AtomicU32 = AtomicU32::new(0);
static CONVERTED: AtomicU32 = AtomicU32::new(0);

fn fpsw() -> u32 {
    let v: u32;
    unsafe { asm!("mvfc fpsw, {}", out(reg) v, options(nomem, nostack)) };
    v
}

// Bits 63-32 and 47-16 of A0 (the RXv1 accumulator).
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

// A1: guard (bits 95-64, sign-extended from bit 71), upper and lower words.
#[cfg(target_feature = "rxv2")]
fn acc1() -> (u32, u32, u32) {
    let (gu, hi, lo): (u32, u32, u32);
    unsafe {
        asm!("mvfacgu #0, a1, {0}", "mvfachi #0, a1, {1}",
             "mvfaclo #0, a1, {2}", out(reg) gu, out(reg) hi, out(reg) lo,
             options(nomem, nostack))
    };
    (gu, hi, lo)
}

#[cfg(target_feature = "rxv2")]
fn set_acc1(gu: u32, hi: u32, lo: u32) {
    unsafe {
        asm!("mvtachi {1}, a1", "mvtacgu {0}, a1", "mvtaclo {2}, a1",
             in(reg) gu, in(reg) hi, in(reg) lo, options(nomem, nostack))
    };
}

// DR0 and DR1 as words, DPSW and DCMR.
#[cfg(target_feature = "dfpu")]
fn dfpu_state() -> [u32; 6] {
    let (h0, l0, h1, l1, dpsw, dcmr): (u32, u32, u32, u32, u32, u32);
    unsafe {
        asm!("dmov.l drh0, {0}", "dmov.l drl0, {1}", "dmov.l drh1, {2}",
             "dmov.l drl1, {3}", "mvfdc dpsw, {4}", "mvfdc dcmr, {5}",
             out(reg) h0, out(reg) l0, out(reg) h1, out(reg) l1,
             out(reg) dpsw, out(reg) dcmr, options(nomem, nostack))
    };
    [h0, l0, h1, l1, dpsw, dcmr]
}

#[cfg(target_feature = "dfpu")]
fn set_dfpu_state(s: [u32; 6]) {
    unsafe {
        asm!("dmov.l {0}, drh0", "dmov.l {1}, drl0", "dmov.l {2}, drh1",
             "dmov.l {3}, drl1", "mvtdc {4}, dpsw", "mvtdc {5}, dcmr",
             in(reg) s[0], in(reg) s[1], in(reg) s[2], in(reg) s[3],
             in(reg) s[4], in(reg) s[5], options(nomem, nostack))
    };
}

// Changes FPSW (rounding towards -infinity), converts with the FPU (itof)
// and overwrites the accumulators and the DFPU registers rx-rt saves.
extern "C" fn soft() {
    ENTRY_FPSW.store(fpsw(), Ordering::Relaxed);
    unsafe { asm!("mvtc #3, fpsw", options(nomem, nostack)) };
    let n = CALLS.load(Ordering::Relaxed);
    CONVERTED.store((core::hint::black_box(n as i32) + 3) as f32 as u32,
                    Ordering::Relaxed);
    set_acc(0xdead_beef, 0x0bad_f00d);
    #[cfg(target_feature = "rxv2")]
    set_acc1(0x0000_0080, 0xfeed_face, 0xcafe_babe);
    #[cfg(target_feature = "dfpu")]
    {
        ENTRY_DPSW.store(dfpu_state()[4], Ordering::Relaxed);
        set_dfpu_state([0xdead, 0xbeef, 0xbad, 0xf00d, 0x103, 0]);
    }
    CALLS.store(n + 1, Ordering::Relaxed);
}
rx_rt::interrupt!(65, soft);

// RXv3e: int Rs reaches the same handler, IPL has 5 bits, and the Protect
// Zone registers read their reset values in the root world.
#[cfg(target_feature = "rxv3e")]
fn rxv3e() {
    let n = CALLS.load(Ordering::Relaxed);
    unsafe { asm!("int {}", in(reg) 65) };
    check(40, CALLS.load(Ordering::Relaxed) == n + 1);
    let psw: u32;
    unsafe {
        asm!("mvtipl #20", "mvfc psw, {}", "mvtipl #0", out(reg) psw,
             options(nomem, nostack))
    };
    check(41, psw >> 24 == 20);
    let (wid, trctb, wpri): (u32, u32, u32);
    unsafe {
        asm!("mvfc wid, {0}", "trmvfc trctb, {1}", "mvtc #0x05000000, wpri",
             "mvfc wpri, {2}", "mvtc #0, wpri", out(reg) wid,
             out(reg) trctb, out(reg) wpri, options(nomem, nostack))
    };
    check(42, wid == 0);
    check(43, trctb == 0x8000_0000);
    check(44, wpri == 0x0500_0000);
}

#[unsafe(no_mangle)]
fn main() -> ! {
    // rx-rt sets FPSW (and DPSW) to 0x100 at reset.
    check(1, fpsw() == 0x100);
    #[cfg(target_feature = "dfpu")]
    check(2, dfpu_state()[4] == 0x100);

    for round in 0..3u32 {
        // Rounding towards zero in the interrupted code. (MVTC cannot set
        // the cause flags, so they are not used.)
        unsafe { asm!("mvtc #0x101, fpsw", options(nomem, nostack)) };
        let hi = 0x1234_5678 ^ round;
        set_acc(hi, 0x9abc_0000);
        #[cfg(target_feature = "rxv2")]
        let a1 = (0x0000_0011 + round, 0x2468_ace0 ^ round, 0x1357_9bdf);
        #[cfg(target_feature = "rxv2")]
        set_acc1(a1.0, a1.1, a1.2);
        #[cfg(target_feature = "dfpu")]
        let dfpu = [0x3ff0_0000 + round, 0x1111_1111, 0x4000_0000,
                    0x2222_2222 ^ round, 0x101, 1];
        #[cfg(target_feature = "dfpu")]
        set_dfpu_state(dfpu);
        let id = 10 * (round + 1);
        check(id, fpsw() == 0x101);
        unsafe { asm!("int #65") };
        check(id + 1, CALLS.load(Ordering::Relaxed) == round + 1);
        check(id + 2, fpsw() == 0x101);
        check(id + 3, acc() == (hi, 0x5678_9abc ^ (round << 16)));
        check(id + 4, ENTRY_FPSW.load(Ordering::Relaxed) == 0x100);
        check(id + 5, CONVERTED.load(Ordering::Relaxed) == round + 3);
        #[cfg(target_feature = "rxv2")]
        check(id + 6, acc1() == a1);
        #[cfg(target_feature = "dfpu")]
        {
            check(id + 7, dfpu_state() == dfpu);
            check(id + 8, ENTRY_DPSW.load(Ordering::Relaxed) == 0x100);
        }
    }

    #[cfg(target_feature = "rxv3e")]
    rxv3e();

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
