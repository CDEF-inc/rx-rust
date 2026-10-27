//! rx72m-hal on the AP-RX72M-0A board: the clock set-up (24 MHz crystal,
//! PLL 240 MHz) and GPIO (the monitor LEDs LD1 / LD2 on PJ0 / PJ1, a
//! pull-up on the unconnected PJ5). The LEDs blink at 5 Hz for about two
//! seconds of CMT0 time (PCLKB / 512), so the run takes about 2 s when PCLKB
//! is 60 MHz. Booted from the code flash by scripts/hw-run.sh (built by
//! scripts/hw-build.sh); the results are left in `checks` / `failures` /
//! `first_failure` / `finished`.

#![no_std]
#![no_main]
// The result variables keep the names the test scripts read.
#![allow(non_upper_case_globals)]

use core::panic::PanicInfo;
use core::ptr::write_volatile;

use embedded_hal::digital::{InputPin, OutputPin, StatefulOutputPin};
use rx72m_hal::clock::Config;
use rx72m_hal::gpio::Parts;
use rx72m_hal::pac::Peripherals;

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut first_failure: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;
// Loop iterations during one CMT0 period (0.1 s), for the ICLK check.
#[unsafe(no_mangle)]
static mut spins: u32 = 0;

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

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();

    // --- clocks
    let clocks = Config::pll_240mhz_from_24mhz().configure(&p.system);
    check(1, clocks.iclk == 240_000_000 && clocks.pclka == 120_000_000);
    check(2, clocks.pclkb == 60_000_000 && clocks.fclk == 60_000_000);
    check(3, p.system.sckcr3().read().cksel().bits() == 0b100);
    let osc = p.system.oscovfsr().read();
    check(4, osc.moovf().bit_is_set() && osc.plovf().bit_is_set());
    check(5, p.system.memwait().read().memwait().bit_is_set());
    check(6, p.system.pllcr().read().stc().bits() == 19);
    check(7, p.system.mofcr().read().modrv2().bits() == 0);
    let sckcr = p.system.sckcr().read();
    check(8, sckcr.ick().bits() == 0 && sckcr.pckb().bits() == 2 && sckcr.pcka().bits() == 1);

    // --- GPIO
    let pins = Parts::take().unwrap();
    check(9, Parts::take().is_none());
    let mut ld1 = pins.pj0.into_push_pull_output();
    let mut ld2 = pins.pj1.into_output(true);
    check(10, ld1.is_low().unwrap() && ld1.is_set_low().unwrap());
    check(11, ld2.is_high().unwrap() && ld2.is_set_high().unwrap());
    ld1.set_high().unwrap();
    check(12, ld1.is_high().unwrap());
    ld2.toggle().unwrap();
    check(13, ld2.is_low().unwrap() && ld2.is_set_low().unwrap());

    // PJ5 goes to the expansion connector only: the pull-up reads high,
    // and driven low it reads low.
    let mut pj5 = pins.pj5.into_pull_up_input();
    check(14, pj5.is_high().unwrap());
    let mut pj5 = pj5.into_push_pull_output();
    check(15, pj5.is_low().unwrap());
    let _ = pj5.into_input();

    // --- CMT0 at PCLKB / 512 = 117187.5 Hz, a compare match every 0.1 s
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.system.mstpcra().modify(|_, w| w.mstpa15().clear_bit());
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) });
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    p.cmt0.cmcor().write(|w| unsafe { w.bits(11_718) });
    p.cmt0.cmcr().write(|w| unsafe { w.cks().bits(3) }.cmie().set_bit());
    p.icu.ir28().write(|w| w.ir().clear_bit());
    p.cmt.cmstr0().write(|w| w.str0().set_bit());

    // ICLK against PCLKB: iterations of a polling loop during one period.
    // At 240 MHz this is in the millions; were the CPU still on LOCO (about
    // 240 kHz), it would be in the thousands.
    let wait_match = || {
        let mut n = 0u32;
        while p.icu.ir28().read().ir().bit_is_clear() {
            n += 1;
        }
        p.icu.ir28().write(|w| w.ir().clear_bit());
        n
    };
    wait_match();
    let n = wait_match();
    unsafe { write_volatile(&raw mut spins, n) };
    check(16, n > 1_000_000);

    // Blink: LD1 at 5 Hz, LD2 opposite, for 20 periods (2 s).
    for _ in 0..20 {
        ld1.toggle().unwrap();
        ld2.toggle().unwrap();
        wait_match();
    }
    check(17, ld1.is_set_high().unwrap() != ld2.is_set_high().unwrap());
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());

    // Leave LD1 on and LD2 off.
    ld1.set_high().unwrap();
    ld2.set_low().unwrap();
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
