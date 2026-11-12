//! The PAC (rx72-pac or rx74-pac, generated at build time from e2 studio's
//! sfrx file) with rx-rt's critical-section on the board: Peripherals::take(),
//! the module stop, CMT and ICU registers through the PAC, a CMT0 compare
//! match interrupt and critical_section::with holding it back. Booted from
//! the code flash / MRAM by scripts/hw-run.sh (scripts/hw-build.sh builds
//! it); the results are left in hw-board's `checks` / `failures` /
//! `first_failure` / `finished`.

#![no_std]
#![no_main]
// rx_rt::interrupt! uses asm!.
#![feature(asm_experimental_arch)]

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use hw_board::check;
use hw_board::hal::pac::Peripherals;
use rx_rt::interrupt;

static TICKS: AtomicU32 = AtomicU32::new(0);

extern "C" fn cmi0() {
    TICKS.store(TICKS.load(Relaxed) + 1, Relaxed);
}
rx_rt::interrupt!(28, cmi0);

/// CMT unit 0 out of module stop: on the RX72M MSTPCRA, write-protected by
/// PRCR; on the RX74N / RX74M MSTPCRF, write-protected by PRCR_S. Whether
/// the bit reads back cleared.
#[cfg(feature = "ap-rx72m")]
fn cmt0_start(p: &Peripherals) -> bool {
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.system.mstpcra().modify(|_, w| w.mstpa15().clear_bit());
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) });
    p.system.mstpcra().read().mstpa15().bit_is_clear()
}
#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
fn cmt0_start(p: &Peripherals) -> bool {
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.mstpreg.mstpcrf().modify(|_, w| w.mstpf15().clear_bit());
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) });
    p.mstpreg.mstpcrf().read().mstpf15().bit_is_clear()
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take();
    check(1, p.is_some());
    check(2, Peripherals::take().is_none());
    let p = p.unwrap();

    check(3, cmt0_start(&p));

    // CMT0: PCLK/512, a compare match every 100 counts, CMIE.
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    p.cmt0.cmcor().write(|w| unsafe { w.bits(99) });
    p.cmt0.cmcr().write(|w| unsafe { w.cks().bits(3) }.cmie().set_bit());
    let cmcr = p.cmt0.cmcr().read();
    check(4, cmcr.cks().bits() == 3 && cmcr.cmie().bit_is_set());
    check(5, p.cmt0.cmcor().read().bits() == 99);

    // The counter runs while started.
    p.cmt.cmstr0().write(|w| w.str0().set_bit());
    let a = p.cmt0.cmcnt().read().bits();
    let mut n = 0u32;
    while p.cmt0.cmcnt().read().bits() == a && n < 10_000_000 {
        n += 1;
    }
    check(6, p.cmt0.cmcnt().read().bits() != a);

    // CMI0 (vector 28) at priority 1: IPR4, IEN4 of IER03 (the RX74's PAC
    // calls it IER3).
    p.icu.ipr4().write(|w| unsafe { w.ipr().bits(1) });
    #[cfg(feature = "ap-rx72m")]
    p.icu.ier03().modify(|_, w| w.ien4().set_bit());
    #[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
    p.icu.ier3().modify(|_, w| w.ien4().set_bit());
    p.icu.ir28().write(|w| w.ir().clear_bit());
    interrupt::enable();
    while TICKS.load(Relaxed) < 5 {
        interrupt::wait();
    }
    check(7, TICKS.load(Relaxed) >= 5);

    // critical_section::with (rx-rt's implementation) clears PSW.I: the
    // compare match stays pending and is taken once, right after.
    let (t, held, pending, masked) = critical_section::with(|_| {
        let t = TICKS.load(Relaxed);
        let mut n = 0u32;
        while p.icu.ir28().read().ir().bit_is_clear() && n < 10_000_000 {
            n += 1;
        }
        (t, TICKS.load(Relaxed), p.icu.ir28().read().ir().bit_is_set(),
         !interrupt::is_enabled())
    });
    check(8, masked);
    check(9, pending && held == t);
    check(10, TICKS.load(Relaxed) == t + 1);
    check(11, interrupt::is_enabled());

    interrupt::disable();
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    hw_board::finish()
}
