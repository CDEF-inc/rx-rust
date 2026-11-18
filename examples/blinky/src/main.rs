//! Blinks two of a board's LEDs forever, alternately at 2 Hz. The board is a
//! feature (exactly one):
//!
//! - `ap-rx72m`: the AP-RX72M-0A's monitor LEDs LD1 / LD2 (PJ0 / PJ1), with
//!   rx72-hal's PLL at 240 MHz.
//! - `ek-rx74n` / `ek-rx74m`: the EK-RX74N's / EK-RX74M's LED1 / LED2 (PF05 /
//!   P807), with CPUCLK0 at 600 MHz.
//!
//! The blinking itself is written against the embedded-hal 1.0 traits only
//! (`blink`); `board::parts` sets up the HAL's clock, pins and CMT2 delay and
//! hands them over.
//!
//! It boots from the code flash / code MRAM through rx-rt, so once written
//! it also runs on its own after a reset or a power cycle, with the debugger
//! cable unplugged from the board (a connected E2 Lite keeps the MCU from
//! running after a reset; docs/NOTES-hw.md). `BLINKS` counts the toggles.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU32, Ordering};

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{OutputPin, StatefulOutputPin};
// The reset handler, vector tables and critical-section implementation.
use rx_rt as _;

#[cfg(not(any(feature = "ap-rx72m", feature = "ek-rx74n", feature = "ek-rx74m")))]
compile_error!("pick the board: --features ap-rx72m, ek-rx74n or ek-rx74m");
#[cfg(any(
    all(feature = "ap-rx72m", any(feature = "ek-rx74n", feature = "ek-rx74m")),
    all(feature = "ek-rx74n", feature = "ek-rx74m")
))]
compile_error!("pick one board only (ap-rx72m, ek-rx74n or ek-rx74m)");

/// The AP-RX72M-0A: LD1 / LD2 on PJ0 / PJ1.
#[cfg(feature = "ap-rx72m")]
mod board {
    use embedded_hal::delay::DelayNs;
    use embedded_hal::digital::{OutputPin, StatefulOutputPin};
    use rx72_hal::clock::Config;
    use rx72_hal::delay::Delay;
    use rx72_hal::gpio::Parts;
    use rx72_hal::pac::Peripherals;

    pub fn parts() -> (impl StatefulOutputPin, impl OutputPin, impl DelayNs) {
        let p = Peripherals::take().unwrap();
        let clocks = Config::pll_240mhz_from_24mhz().configure(&p.system);
        let pins = Parts::take().unwrap();
        let delay = Delay::new(p.cmt2, &clocks);
        (pins.pj0.into_push_pull_output(), pins.pj1.into_push_pull_output(), delay)
    }
}

/// The EK-RX74N / EK-RX74M: LED1 / LED2 on PF05 / P807.
#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
mod board {
    use embedded_hal::delay::DelayNs;
    use embedded_hal::digital::{OutputPin, StatefulOutputPin};
    use rx74_hal::clock::Config;
    use rx74_hal::delay::Delay;
    use rx74_hal::gpio::Parts;
    use rx74_hal::pac::Peripherals;

    pub fn parts() -> (impl StatefulOutputPin, impl OutputPin, impl DelayNs) {
        let p = Peripherals::take().unwrap();
        let clocks = Config::pll_600mhz_from_24mhz().configure(&p.sysc, &p.sram, &p.mram);
        let pins = Parts::take().unwrap();
        let delay = Delay::new(p.cmt2, &clocks);
        (pins.pf05.into_push_pull_output(), pins.p807.into_push_pull_output(), delay)
    }
}

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
    let (mut a, mut b, mut delay) = board::parts();
    blink(&mut a, &mut b, &mut delay, 250)
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}
