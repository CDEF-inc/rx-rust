//! The HAL's interrupt controller API (`hal::icu`) on the board: the
//! software interrupt SWINT (vector 27, IPR3) and CMT0 / CMT1 compare
//! matches (vectors 28 / 29, IPR4 / IPR5) through `Interrupt`: enable,
//! priority, request flag, priority against PSW.IPL, and a higher-priority
//! source taken inside a lower one's handler. A wrong IPR in a constant
//! leaves that source at priority 0, never accepted, and fails a check.
//! Booted from the code flash / MRAM by scripts/hw-run.sh (scripts/hw-build.sh
//! builds it); the results are left in hw-board's `checks` / `failures` /
//! `first_failure` / `finished`.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use hw_board::check;
use hw_board::hal::icu::{self, Interrupt, Priority};
use hw_board::hal::pac::Peripherals;
use rx_rt::interrupt;

static SWINTS: AtomicU32 = AtomicU32::new(0);
static CMI0S: AtomicU32 = AtomicU32::new(0);
static CMI1S: AtomicU32 = AtomicU32::new(0);
/// CMI1s counted inside the SWINT handler (nested).
static NESTED: AtomicU32 = AtomicU32::new(0);
/// Set while main wants the SWINT handler to wait for a CMI1.
static WAIT_IN_SWINT: AtomicU32 = AtomicU32::new(0);

extern "C" fn swint() {
    SWINTS.store(SWINTS.load(Relaxed) + 1, Relaxed);
    if WAIT_IN_SWINT.load(Relaxed) != 0 {
        // Allow nesting: only a source above this one's priority (IPL)
        // gets in.
        interrupt::enable();
        let before = CMI1S.load(Relaxed);
        let mut n = 0u32;
        while CMI1S.load(Relaxed) == before && n < SPINS {
            n += 1;
        }
        if CMI1S.load(Relaxed) != before {
            NESTED.store(NESTED.load(Relaxed) + 1, Relaxed);
        }
        interrupt::disable();
    }
}
rx_rt::interrupt!(27, swint);

extern "C" fn cmi0() {
    CMI0S.store(CMI0S.load(Relaxed) + 1, Relaxed);
}
rx_rt::interrupt!(28, cmi0);

extern "C" fn cmi1() {
    CMI1S.store(CMI1S.load(Relaxed) + 1, Relaxed);
}
rx_rt::interrupt!(29, cmi1);

/// CMT unit 0 (CMT0 / CMT1) out of module stop.
#[cfg(feature = "ap-rx72m")]
fn cmt_unit0_start(p: &Peripherals) {
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.system.mstpcra().modify(|_, w| w.mstpa15().clear_bit());
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) });
}
#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
fn cmt_unit0_start(p: &Peripherals) {
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.mstpreg.mstpcrf().modify(|_, w| w.mstpf15().clear_bit());
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) });
}

fn set_ipl(ipl: u8) {
    // mvtipl takes an immediate.
    unsafe {
        match ipl {
            0 => asm!("mvtipl #0"),
            1 => asm!("mvtipl #1"),
            2 => asm!("mvtipl #2"),
            _ => asm!("mvtipl #15"),
        }
    }
}

/// How long the waits spin. The test leaves the clocks as a reset does:
/// the RX72M runs on the 240 kHz LOCO, where this is about 4 s, and the
/// RX74N / RX74M on the 8 MHz MOCO. CMT at PCLK / 512 with 100 counts per
/// compare match then gives one every 0.2 s (RX72M) or 6.4 ms.
const SPINS: u32 = 200_000;

/// Spins until `f` holds or [`SPINS`] are over; whether it held.
fn wait_for(f: impl Fn() -> bool) -> bool {
    let mut n = 0u32;
    while !f() && n < SPINS {
        n += 1;
    }
    f()
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let sw = Interrupt::SWINT;
    check(1, sw.vector() == 27 && Interrupt::CMI0.vector() == 28);

    // Priority and enable round trips.
    sw.set_priority(Priority::P2);
    check(2, sw.priority() == Some(Priority::P2));
    check(3, Priority::new(0).is_none() && Priority::new(16).is_none());
    sw.disable();
    check(4, !sw.is_enabled());
    sw.enable();
    check(5, sw.is_enabled());
    sw.disable();

    // SWINT with IEN clear: IR is set and stays; nothing is taken.
    sw.clear_pending();
    check(6, !sw.is_pending());
    interrupt::enable();
    icu::trigger_swint();
    check(7, sw.is_pending());
    check(8, SWINTS.load(Relaxed) == 0);
    sw.clear_pending();
    check(9, !sw.is_pending());

    // Enabled: taken, and accepting it clears IR.
    sw.enable();
    icu::trigger_swint();
    check(10, wait_for(|| SWINTS.load(Relaxed) == 1));
    check(11, !sw.is_pending());

    // Priority against PSW.IPL: at IPL 2 a priority-2 request waits, and
    // is taken once its priority goes above the IPL.
    set_ipl(2);
    icu::trigger_swint();
    check(12, !wait_for(|| SWINTS.load(Relaxed) == 2) && sw.is_pending());
    sw.set_priority(Priority::new(3).unwrap());
    check(13, wait_for(|| SWINTS.load(Relaxed) == 2));
    set_ipl(0);

    // CMT0 / CMT1 at PCLK/512, compare matches every 100 counts. CMI0
    // through its constant (IPR4): taken.
    cmt_unit0_start(&p);
    p.cmt.cmstr0().write(|w| w.str0().clear_bit().str1().clear_bit());
    p.cmt0.cmcor().write(|w| unsafe { w.bits(99) });
    p.cmt0.cmcr().write(|w| unsafe { w.cks().bits(3) }.cmie().set_bit());
    p.cmt1.cmcor().write(|w| unsafe { w.bits(99) });
    p.cmt1.cmcr().write(|w| unsafe { w.cks().bits(3) }.cmie().set_bit());
    let cmi0 = Interrupt::CMI0;
    cmi0.set_priority(Priority::P1);
    cmi0.clear_pending();
    cmi0.enable();
    p.cmt.cmstr0().write(|w| w.str0().set_bit());
    check(14, wait_for(|| CMI0S.load(Relaxed) >= 3));
    cmi0.disable();
    let n = CMI0S.load(Relaxed);
    check(15, !wait_for(|| CMI0S.load(Relaxed) > n + 1));
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());

    // Nesting: SWINT at priority 3 waits in its handler with interrupts on;
    // CMI1 (IPR5) at priority 7 gets in, a CMI1 at priority 2 does not.
    let cmi1 = Interrupt::CMI1;
    cmi1.set_priority(Priority::P7);
    cmi1.clear_pending();
    cmi1.enable();
    WAIT_IN_SWINT.store(1, Relaxed);
    interrupt::disable();
    p.cmt.cmstr0().write(|w| w.str1().set_bit());
    icu::trigger_swint();
    interrupt::enable();
    check(16, wait_for(|| SWINTS.load(Relaxed) == 3));
    check(17, NESTED.load(Relaxed) == 1);
    cmi1.set_priority(Priority::P2);
    icu::trigger_swint();
    check(18, wait_for(|| SWINTS.load(Relaxed) == 4));
    check(19, NESTED.load(Relaxed) == 1);
    // The held-back CMI1s were taken once SWINT returned.
    check(20, CMI1S.load(Relaxed) > 0);
    WAIT_IN_SWINT.store(0, Relaxed);

    interrupt::disable();
    p.cmt.cmstr0().write(|w| w.str0().clear_bit().str1().clear_bit());
    cmi1.disable();
    sw.disable();
    hw_board::finish()
}
