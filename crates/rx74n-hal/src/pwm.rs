//! PWM on GPTW8 in sawtooth-wave PWM mode 1, out of GTIOC8B on PF05: the
//! EK-RX74N's user LED LED1 (green), as an embedded-hal `SetDutyCycle`.
//!
//! GPTW8 counts up from 0 to GTPR and starts again, so the period is GTPR +
//! 1 counts. GTIOC8B goes high at the end of each cycle and low on the
//! GTCCRB compare match (GTIOR.GTIOB = 01001b, manual table 24.4): the
//! output is high for GTCCRB counts at the start of each period. Duty 0 and
//! full are the forced levels of GTUDDTYC.OBDTY. A new duty cycle goes to
//! GTCCRE, GTCCRB's buffer (GTBER.CCRB = 01b), which takes effect at the
//! end of the cycle, so there are no odd pulses.
//!
//! The count clock (GTCLK) is PCLKD: GTCLKCR.BPEN = 1, the synchronous
//! clock, set before the GPTW leaves module stop (the asynchronous one,
//! GPTCLK, is not set up by [`crate::clock`]). It goes through a prescaler
//! of GTCLK / 1, 2, 4, ... 1024 (GTCR.TPCS), the smallest that fits the
//! period in at most 65535 counts, for the finest duty steps that fit
//! `SetDutyCycle`'s u16.
//!
//! Registers at GPTW8's secure addresses (0x8742C800), addressed directly.

use core::convert::Infallible;
use core::ptr::{read_volatile, write_volatile};

use embedded_hal::pwm::{ErrorType, SetDutyCycle};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::mstp::{self, Mstpcr};
use crate::pac;

const BASE: usize = 0x8742_c800;
const GTCR: *mut u32 = (BASE + 0x2c) as _;
const GTUDDTYC: *mut u32 = (BASE + 0x30) as _;
const GTIOR: *mut u32 = (BASE + 0x34) as _;
const GTBER: *mut u32 = (BASE + 0x40) as _;
const GTCNT: *mut u32 = (BASE + 0x48) as _;
const GTCCRB: *mut u32 = (BASE + 0x50) as _;
const GTCCRE: *mut u32 = (BASE + 0x58) as _;
const GTPR: *mut u32 = (BASE + 0x64) as _;
const GTPBR: *mut u32 = (BASE + 0x68) as _;
const GTCLKCR: *mut u32 = 0x8742_bf10_usize as _;
const MSTPCRE: *const u32 = 0x8780_3010_usize as _;

// GTCR
const CST: u32 = 1 << 0;
const TPCS_SHIFT: u32 = 23;
// GTUDDTYC: count up; OBDTY = 10b (0 %) or 11b (100 %) forces GTIOCnB.
const UD: u32 = 1 << 0;
const OBDTY_0: u32 = 0b10 << 24;
const OBDTY_100: u32 = 0b11 << 24;
// GTIOR: GTIOCnB output enabled, initial low, high at the end of a cycle,
// low on the GTCCRB compare match.
const GTIOR_B: u32 = 1 << 24 | 0b01001 << 16;
// GTBER.CCRB = 01b: GTCCRE is GTCCRB's single buffer.
const GTBER_CCRB_SINGLE: u32 = 0b01 << 18;

/// PSEL of GTIOC8B on PF05.
const PSEL_GTIOC8B: u8 = 0b000_0011;

/// The timing of a PWM frequency: the prescaler and the period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// The count clock divider (GTCLK / 2^`tpcs`).
    pub tpcs: u8,
    /// The period in counts (GTPR + 1), 2 to 65535.
    pub period: u16,
    /// The resulting frequency in Hz (rounded down).
    pub hz: u32,
}

impl Timing {
    /// The smallest prescaler whose period for `hz` (rounded to the
    /// nearest count) fits in 65535 counts. `None` if `hz` is 0, above
    /// GTCLK / 2 or below about GTCLK / (1024 x 65535) (1.8 Hz at 120 MHz).
    pub fn new(gtclk: u32, hz: u32) -> Option<Self> {
        if hz == 0 {
            return None;
        }
        for tpcs in 0..=10u8 {
            let div = 1u64 << tpcs;
            let period = (gtclk as u64 + div * hz as u64 / 2) / (div * hz as u64);
            if period > 65535 {
                continue;
            }
            if period < 2 {
                return None;
            }
            let hz = (gtclk as u64 / (div * period)) as u32;
            return Some(Timing { tpcs, period: period as u16, hz });
        }
        None
    }
}

/// GPTW8 as a PWM output on PF05 (GTIOC8B).
pub struct Pwm {
    gpt: pac::Gptw8,
    timing: Timing,
    pin: Pin<0x0f, 5, Function>,
}

impl Pwm {
    /// Sets up GPTW8 (GTCLKCR.BPEN, then the GPTW out of module stop,
    /// MSTPCRE.MSTPE31) in sawtooth-wave PWM mode 1 at the frequency
    /// nearest `hz` of PCLKD and starts it with duty 0 (low), then switches
    /// PF05 to GTIOC8B.
    ///
    /// Panics if `hz` is out of range (see [`Timing::new`]), or if the GPTW
    /// already left module stop with the asynchronous clock (GTCLKCR can
    /// only change before).
    pub fn gptw8<M>(gpt: pac::Gptw8, pin: Pin<0x0f, 5, M>, hz: u32, clocks: &Clocks) -> Self {
        let timing = Timing::new(clocks.pclkd, hz).expect("PWM frequency out of range");
        unsafe {
            if read_volatile(MSTPCRE) & 1 << 31 != 0 {
                write_volatile(GTCLKCR, 1);
                mstp::start(Mstpcr::E, 31);
            }
            assert!(read_volatile(GTCLKCR) & 1 != 0, "GPTW running on the asynchronous clock");
            // Stopped, sawtooth-wave PWM mode 1, the prescaler.
            write_volatile(GTCR, (timing.tpcs as u32) << TPCS_SHIFT);
            write_volatile(GTUDDTYC, UD | OBDTY_0);
            write_volatile(GTIOR, GTIOR_B);
            write_volatile(GTBER, GTBER_CCRB_SINGLE);
            write_volatile(GTPR, timing.period as u32 - 1);
            write_volatile(GTPBR, timing.period as u32 - 1);
            write_volatile(GTCCRB, 0);
            write_volatile(GTCCRE, 0);
            write_volatile(GTCNT, 0);
            write_volatile(GTCR, (timing.tpcs as u32) << TPCS_SHIFT | CST);
        }
        let pin = pin.into_function(PSEL_GTIOC8B);
        Pwm { gpt, timing, pin }
    }

    /// The prescaler, the period and the frequency in use.
    pub fn timing(&self) -> Timing {
        self.timing
    }

    /// Stops GPTW8 (the pin goes low, GTIOR.OBDFLT) and gives it and the
    /// pin back.
    pub fn free(self) -> (pac::Gptw8, Pin<0x0f, 5, Function>) {
        unsafe { write_volatile(GTCR, read_volatile(GTCR) & !CST) };
        (self.gpt, self.pin)
    }
}

impl ErrorType for Pwm {
    type Error = Infallible;
}

impl SetDutyCycle for Pwm {
    /// The period in counts.
    fn max_duty_cycle(&self) -> u16 {
        self.timing.period
    }

    /// Sets the high time to `duty` counts of the period (clamped to it),
    /// from the end of the current cycle.
    fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Infallible> {
        let duty = duty.min(self.timing.period);
        unsafe {
            if duty == 0 {
                write_volatile(GTUDDTYC, UD | OBDTY_0);
            } else if duty == self.timing.period {
                write_volatile(GTUDDTYC, UD | OBDTY_100);
            } else {
                write_volatile(GTCCRE, duty as u32);
                write_volatile(GTUDDTYC, UD);
            }
        }
        Ok(())
    }
}
