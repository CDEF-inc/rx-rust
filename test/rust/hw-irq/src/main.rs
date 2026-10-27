//! Interrupts from a peripheral through rx-rt on the RX72M board: CMT0 and
//! CMT1 compare match interrupts (vectors 28 and 29), a software interrupt,
//! `interrupt::free` holding an interrupt back, PSW.IPL masking and a
//! higher-priority interrupt nesting in a handler that re-enables
//! interrupts. Booted from the code flash by scripts/hw-run.sh (built by
//! scripts/hw-build.sh); the results are left in `checks` / `failures` /
//! `first_failure` / `finished`, and the run ends at `done`.

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]
// The result variables keep the names the test scripts read.
#![allow(non_upper_case_globals)]

use core::arch::asm;
use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use rx_rt::interrupt;

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

// RX72M registers.
mod reg {
    pub const PRCR: *mut u16 = 0x0008_03fe as _;
    pub const MSTPCRA: *mut u32 = 0x0008_0010 as _;
    pub const CMSTR0: *mut u16 = 0x0008_8000 as _;
    pub const CMCR0: *mut u16 = 0x0008_8002 as _;
    pub const CMCOR0: *mut u16 = 0x0008_8006 as _;
    pub const CMCR1: *mut u16 = 0x0008_8008 as _;
    pub const CMCOR1: *mut u16 = 0x0008_800c as _;
    // ICU: IR and IER bits of vectors 28 (CMI0) and 29 (CMI1). Their
    // priorities are IPR004 and IPR005, not IPR028 / IPR029: the IPR
    // registers of the low vectors go by peripheral, as on the RX62N.
    pub const IR028: *mut u8 = 0x0008_701c as _;
    pub const IER03: *mut u8 = 0x0008_7203 as _;
    pub const IPR004: *mut u8 = 0x0008_7304 as _;
    pub const IPR005: *mut u8 = 0x0008_7305 as _;
}

fn rd<T>(p: *mut T) -> T {
    unsafe { read_volatile(p) }
}
fn wr<T>(p: *mut T, v: T) {
    unsafe { write_volatile(p, v) }
}

static SOFT: AtomicU32 = AtomicU32::new(0);
static TICKS0: AtomicU32 = AtomicU32::new(0);
static TICKS1: AtomicU32 = AtomicU32::new(0);
// 1: the next CMT0 interrupt re-enables interrupts and waits for CMT1;
// 2: it is doing so; 3: done.
static NEST: AtomicU32 = AtomicU32::new(0);
static NESTED: AtomicU32 = AtomicU32::new(0);

extern "C" fn soft() {
    SOFT.store(SOFT.load(Relaxed) + 1, Relaxed);
}
rx_rt::interrupt!(64, soft);

extern "C" fn cmi0() {
    TICKS0.store(TICKS0.load(Relaxed) + 1, Relaxed);
    if NEST.load(Relaxed) == 1 {
        NEST.store(2, Relaxed);
        // PSW.IPL is 1 here, so only CMT1 (priority 2) gets in.
        let t1 = TICKS1.load(Relaxed);
        interrupt::enable();
        let mut n = 0u32;
        while TICKS1.load(Relaxed) == t1 && n < 1_000_000 {
            n += 1;
        }
        interrupt::disable();
        NESTED.store((TICKS1.load(Relaxed) != t1) as u32, Relaxed);
        NEST.store(3, Relaxed);
    }
}
rx_rt::interrupt!(28, cmi0);

extern "C" fn cmi1() {
    TICKS1.store(TICKS1.load(Relaxed) + 1, Relaxed);
}
rx_rt::interrupt!(29, cmi1);

// Spins until the CMT0 compare match is pending (IR028), bounded.
fn wait_pending() -> bool {
    let mut n = 0u32;
    while rd(reg::IR028) == 0 {
        n += 1;
        if n == 10_000_000 {
            return false;
        }
    }
    true
}

#[unsafe(no_mangle)]
fn main() -> ! {
    // CMT unit 0 out of module stop (MSTPCRA is write-protected by PRCR).
    wr(reg::PRCR, 0xa502);
    wr(reg::MSTPCRA, rd(reg::MSTPCRA) & !(1 << 15));
    wr(reg::PRCR, 0xa500);
    wr(reg::CMSTR0, 0);

    // A software interrupt is taken even with PSW.I clear.
    unsafe { asm!("int #64") };
    check(1, SOFT.load(Relaxed) == 1);
    check(2, !interrupt::is_enabled());

    // CMT0: PCLK/512, a compare match every 100 counts, priority 1. CMT1:
    // every 50 counts, priority 2 (started for the nesting test). PCLK/512
    // keeps the period well above the time from re-enabling to the checks
    // also in QEMU (PCLK 48 MHz there, 240 kHz LOCO on the board).
    wr(reg::CMCOR0, 99);
    wr(reg::CMCR0, 1 << 6 | 3);
    wr(reg::CMCOR1, 49);
    wr(reg::CMCR1, 1 << 6 | 3);
    wr(reg::IPR004, 1);
    wr(reg::IPR005, 2);
    wr(reg::IER03, rd(reg::IER03) | 3 << 4);
    wr(reg::CMSTR0, 1);
    interrupt::enable();
    while TICKS0.load(Relaxed) < 5 {
        interrupt::wait();
    }
    check(3, interrupt::is_enabled());
    check(4, TICKS1.load(Relaxed) == 0);

    // interrupt::free holds the compare match back; it is taken once, right
    // after.
    let (t, held, pending) = interrupt::free(|| {
        let t = TICKS0.load(Relaxed);
        let pending = wait_pending() && wait_pending();
        (t, TICKS0.load(Relaxed), pending)
    });
    check(5, pending && held == t);
    check(6, TICKS0.load(Relaxed) == t + 1);
    check(7, interrupt::is_enabled());

    // PSW.IPL = 1 masks the priority-1 interrupt.
    unsafe { asm!("mvtipl #1") };
    let t = TICKS0.load(Relaxed);
    let pending = wait_pending();
    let held = TICKS0.load(Relaxed);
    unsafe { asm!("mvtipl #0") };
    check(8, pending && held == t);
    check(9, TICKS0.load(Relaxed) == t + 1);

    // Nesting: CMT1 interrupts the CMT0 handler once it sets PSW.I.
    NEST.store(1, Relaxed);
    wr(reg::CMSTR0, 3);
    while NEST.load(Relaxed) != 3 {
        interrupt::wait();
    }
    check(10, NESTED.load(Relaxed) == 1);
    check(11, TICKS1.load(Relaxed) >= 1);

    interrupt::disable();
    wr(reg::CMSTR0, 0);
    unsafe { write_volatile(&raw mut finished, 1) };
    done()
}

// The end of the test (the board runner's breakpoint).
#[unsafe(no_mangle)]
#[inline(never)]
extern "C" fn done() -> ! {
    loop {
        interrupt::wait();
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
