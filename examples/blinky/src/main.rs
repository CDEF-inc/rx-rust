//! Blinks the AP-RX72M-0A's monitor LEDs LD1 / LD2 (PJ0 / PJ1) forever, LD1
//! and LD2 alternately at 2 Hz. The blinking itself is written against the
//! embedded-hal 1.0 traits only (`blink`); `main` sets up rx72m-hal's
//! clock (PLL 240 MHz), pins and CMT2 delay and hands them over.
//!
//! It boots from the code flash through rx-rt, so once written it also runs
//! on its own after a reset or a power cycle. `blinks` counts the toggles.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU32, Ordering};

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{OutputPin, StatefulOutputPin};
use rx72m_hal::clock::Config;
use rx72m_hal::delay::Delay;
use rx72m_hal::gpio::Parts;
use rx72m_hal::pac::Peripherals;
// The reset handler, vector tables and critical-section implementation.
use rx_rt as _;

/// LED toggles so far (to see from a debugger that it runs).
#[unsafe(no_mangle)]
static BLINKS: AtomicU32 = AtomicU32::new(0);

/// Toggles `a`, drives `b` to the opposite level, waits `ms`; forever.
fn blink(
    a: &mut impl StatefulOutputPin,
    b: &mut impl OutputPin,
    delay: &mut impl DelayNs,
    ms: u32,
) -> ! {
    loop {
        a.toggle().unwrap();
        b.set_state((!a.is_set_high().unwrap()).into()).unwrap();
        BLINKS.fetch_add(1, Ordering::Relaxed);
        delay.delay_ms(ms);
    }
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = Config::pll_240mhz_from_24mhz().configure(&p.system);
    let pins = Parts::take().unwrap();
    let mut ld1 = pins.pj0.into_push_pull_output();
    let mut ld2 = pins.pj1.into_push_pull_output();
    let mut delay = Delay::new(p.cmt2, &clocks);
    blink(&mut ld1, &mut ld2, &mut delay, 250)
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}
