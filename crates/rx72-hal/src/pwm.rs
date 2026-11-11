//! PWM on MTU6 in PWM mode 1, out of MTIOC6A on PJ1: the AP-RX72M-0A's
//! monitor LED LD2 (lit when high), as an embedded-hal `SetDutyCycle`.
//!
//! MTU6 counts PCLKA (120 MHz) through a prescaler and clears on the TGRA
//! compare match, so the period is TGRA + 1 counts. MTIOC6A goes high on
//! the TGRB match and low on the TGRA match: the output is high for TGRA -
//! TGRB counts at the end of each period (manual figure 24.27, with the
//! levels the other way round). Duty 0 and full are the constant levels of
//! TIOR (low or high on both matches) with TGRB = 0: a TGRB equal to TGRA
//! would leave the output unchanged (figure 24.29), also when TIOR changes
//! to the other level. The prescaler is the smallest of
//! PCLKA / 1, 2, 4, 8, 16, 32, 64, 256, 1024 that fits the period in at
//! most 65535 counts, for the finest duty steps.
//!
//! TGRB is not buffered: a new duty cycle takes effect at its compare
//! match, so the period it is written in can have one odd pulse.
//!
//! MTU6's registers are addressed directly (0xC1A00; TSTRB, shared with
//! MTU7, is changed in a critical section).

use core::convert::Infallible;
use core::ptr::{read_volatile, write_volatile};

use embedded_hal::pwm::{ErrorType, SetDutyCycle};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::mstp::{self, Mstpcr};
use crate::pac;

const TCR: *mut u8 = 0x000c_1a00 as _;
const TMDR1: *mut u8 = 0x000c_1a02 as _;
const TIORH: *mut u8 = 0x000c_1a04 as _;
const TCNT: *mut u16 = 0x000c_1a10 as _;
const TGRA: *mut u16 = 0x000c_1a18 as _;
const TGRB: *mut u16 = 0x000c_1a1a as _;
const TCR2: *mut u8 = 0x000c_1a4c as _;
const TSTRB: *mut u8 = 0x000c_1a80 as _;
const CST6: u8 = 1 << 6;

/// PJ1PFS.PSEL of MTIOC6A.
const PSEL_MTIOC6A: u8 = 0b000001;

// TIOR codes (manual table 24.35): output at the compare match.
const IO_LOW: u8 = 0b0001; // initial low, low at the match
const IO_HIGH_AT_MATCH: u8 = 0b0010; // initial low, high at the match
const IO_HIGH: u8 = 0b0110; // initial high, high at the match

/// The count clock dividers of MTU6 and their TCR.TPSC / TCR2.TPSC2
/// settings (manual table 24.9), smallest first.
const PRESCALERS: [(u32, u8, u8); 9] = [
    (1, 0b000, 0),
    (2, 0, 0b001),
    (4, 0b001, 0),
    (8, 0, 0b010),
    (16, 0b010, 0),
    (32, 0, 0b011),
    (64, 0b011, 0),
    (256, 0b100, 0),
    (1024, 0b101, 0),
];

/// The timing of a PWM frequency: the prescaler and the period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// The count clock divider (PCLKA / `div`).
    pub div: u32,
    /// The period in counts (TGRA + 1), 2 to 65535.
    pub period: u16,
    /// The resulting frequency in Hz (rounded down).
    pub hz: u32,
}

impl Timing {
    /// The smallest prescaler whose period for `hz` (rounded to the
    /// nearest count) fits in 65535 counts. `None` if `hz` is 0, above
    /// PCLKA / 2 or below about PCLKA / (1024 x 65535) (1.8 Hz at 120 MHz).
    pub fn new(pclk: u32, hz: u32) -> Option<Self> {
        if hz == 0 {
            return None;
        }
        for (div, _, _) in PRESCALERS {
            let period = (pclk as u64 + (div * hz) as u64 / 2) / (div as u64 * hz as u64);
            if period > 65535 {
                continue;
            }
            if period < 2 {
                return None;
            }
            let hz = (pclk as u64 / (div as u64 * period)) as u32;
            return Some(Timing { div, period: period as u16, hz });
        }
        None
    }
}

/// MTU6 as a PWM output on PJ1 (MTIOC6A).
pub struct Pwm {
    mtu: pac::Mtu6,
    timing: Timing,
    pin: Pin<0x12, 1, Function>,
}

impl Pwm {
    /// Sets up MTU6 (taking the MTU out of module stop, MSTPCRA.MSTPA9) in
    /// PWM mode 1 at the frequency nearest `hz` and starts it with duty 0
    /// (low), then switches PJ1 to MTIOC6A.
    ///
    /// Panics if `hz` is out of range (see [`Timing::new`]).
    pub fn mtu6<M>(mtu: pac::Mtu6, pin: Pin<0x12, 1, M>, hz: u32, clocks: &Clocks) -> Self {
        let timing = Timing::new(clocks.pclka, hz).expect("PWM frequency out of range");
        let &(_, tpsc, tpsc2) = PRESCALERS.iter().find(|p| p.0 == timing.div).unwrap();
        mstp::start(Mstpcr::A, 9);
        set_running(false);
        unsafe {
            // Cleared by the TGRA compare match, rising edge of the clock.
            write_volatile(TCR, 0b001 << 5 | tpsc);
            write_volatile(TCR2, tpsc2);
            // PWM mode 1, no buffers.
            write_volatile(TMDR1, 0b0010);
            write_volatile(TGRA, timing.period - 1);
            write_volatile(TGRB, 0);
            // Duty 0: low at both matches (and low now, written stopped).
            write_volatile(TIORH, IO_LOW << 4 | IO_LOW);
            write_volatile(TCNT, 0);
        }
        set_running(true);
        let pin = pin.into_function(PSEL_MTIOC6A);
        Pwm { mtu, timing, pin }
    }

    /// The prescaler, the period and the frequency in use.
    pub fn timing(&self) -> Timing {
        self.timing
    }

    /// Stops MTU6 (the pin keeps its last level) and gives it and the pin
    /// back.
    pub fn free(self) -> (pac::Mtu6, Pin<0x12, 1, Function>) {
        set_running(false);
        (self.mtu, self.pin)
    }
}

/// Starts or stops MTU6.TCNT (TSTRB.CST6).
fn set_running(on: bool) {
    critical_section::with(|_| unsafe {
        let v = read_volatile(TSTRB);
        write_volatile(TSTRB, if on { v | CST6 } else { v & !CST6 });
    });
}

impl ErrorType for Pwm {
    type Error = Infallible;
}

impl SetDutyCycle for Pwm {
    /// The period in counts.
    fn max_duty_cycle(&self) -> u16 {
        self.timing.period
    }

    /// Sets the high time to `duty` counts of the period (clamped to it).
    fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Infallible> {
        let period = self.timing.period;
        let duty = duty.min(period);
        unsafe {
            if duty == 0 || duty == period {
                // The same level at both matches; TGRB apart from TGRA so
                // that the matches set it.
                let io = if duty == 0 { IO_LOW } else { IO_HIGH };
                write_volatile(TGRB, 0);
                write_volatile(TIORH, io << 4 | io);
            } else {
                // High from the TGRB match to the TGRA match (TGRA = period - 1).
                write_volatile(TGRB, period - 1 - duty);
                write_volatile(TIORH, IO_HIGH_AT_MATCH << 4 | IO_LOW);
            }
        }
        Ok(())
    }
}
