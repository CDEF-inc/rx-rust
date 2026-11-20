//! The watchdog timers in register start mode: the independent watchdog
//! [`Iwdt`] (its own 120 kHz clock, IWDTCLK) and the watchdog [`Wdt`]
//! (PCLKB). Each counts down from its timeout; [`Iwdt::feed`] /
//! [`Wdt::feed`] (00h then FFh to the refresh register) starts it over.
//! At the underflow, or at a feed outside the window, it resets the MCU
//! (or requests an interrupt, with `reset: false`), and
//! `reset::cause().iwdt()` / `wdt()` say so after the reset.
//!
//! Register start mode is the default of the option-setting memory (OFS0
//! all ones). The settings can be written once after a reset, and the
//! first feed starts the counter; neither can be stopped afterwards. In
//! auto-start mode (OFS0) the counter already runs with OFS0's settings,
//! and `start` reports [`StartError::Locked`].

use core::ptr::{read_volatile, write_volatile};

use crate::pac;

/// Why `start` failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StartError {
    /// The settings did not take (they are written once after a reset, and
    /// OFS0's auto-start mode uses its own): the timer is not this one's.
    Locked,
    /// The IWDT's clock did not start.
    Clock,
}

impl core::fmt::Display for StartError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            StartError::Locked => "watchdog settings already fixed",
            StartError::Clock => "IWDT clock not running",
        })
    }
}

impl core::error::Error for StartError {}

/// The timeout in cycles of the divided clock (TOPS).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Timeout {
    Cycles1024 = 0,
    Cycles4096 = 1,
    Cycles8192 = 2,
    Cycles16384 = 3,
}

impl Timeout {
    pub const fn cycles(self) -> u32 {
        match self {
            Timeout::Cycles1024 => 1024,
            Timeout::Cycles4096 => 4096,
            Timeout::Cycles8192 => 8192,
            Timeout::Cycles16384 => 16384,
        }
    }
}

/// Where the refresh window starts, as the part of the timeout left (RPSS):
/// a feed before it is a refresh error. 100% is no limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum WindowStart {
    Left25 = 0,
    Left50 = 1,
    Left75 = 2,
    Left100 = 3,
}

/// Where the refresh window ends, as the part of the timeout left (RPES):
/// a feed after it is a refresh error. 0% is no limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum WindowEnd {
    Left75 = 0,
    Left50 = 1,
    Left25 = 2,
    Left0 = 3,
}

/// The IWDT clock divider (CKS) for its 120 kHz clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum IwdtDiv {
    Div1 = 0b0000,
    Div16 = 0b0010,
    Div32 = 0b0011,
    Div64 = 0b0100,
    Div128 = 0b1111,
    Div256 = 0b0101,
}

impl IwdtDiv {
    pub const fn divisor(self) -> u32 {
        match self {
            IwdtDiv::Div1 => 1,
            IwdtDiv::Div16 => 16,
            IwdtDiv::Div32 => 32,
            IwdtDiv::Div64 => 64,
            IwdtDiv::Div128 => 128,
            IwdtDiv::Div256 => 256,
        }
    }
}

/// The WDT clock divider (CKS) for PCLKB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum WdtDiv {
    Div4 = 0b0001,
    Div64 = 0b0100,
    Div128 = 0b1111,
    Div512 = 0b0110,
    Div2048 = 0b0111,
    Div8192 = 0b1000,
}

impl WdtDiv {
    pub const fn divisor(self) -> u32 {
        match self {
            WdtDiv::Div4 => 4,
            WdtDiv::Div64 => 64,
            WdtDiv::Div128 => 128,
            WdtDiv::Div512 => 512,
            WdtDiv::Div2048 => 2048,
            WdtDiv::Div8192 => 8192,
        }
    }
}

/// A watchdog's settings, with `D` its divider type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config<D> {
    pub div: D,
    pub timeout: Timeout,
    pub window_start: WindowStart,
    pub window_end: WindowEnd,
    /// Reset at the underflow / refresh error (RSTIRQS = 1); else request
    /// the interrupt (NMI or a peripheral interrupt).
    pub reset: bool,
}

impl<D> Config<D> {
    /// `div` and `timeout`, no window, a reset.
    pub const fn new(div: D, timeout: Timeout) -> Self {
        Config {
            div,
            timeout,
            window_start: WindowStart::Left100,
            window_end: WindowEnd::Left0,
            reset: true,
        }
    }
}

impl Config<IwdtDiv> {
    /// The timeout in microseconds at the nominal 120 kHz.
    pub const fn timeout_us(&self) -> u32 {
        (self.timeout.cycles() as u64 * self.div.divisor() as u64 * 1_000_000 / 120_000) as u32
    }
}

impl Config<WdtDiv> {
    /// The timeout in microseconds at PCLKB `pclkb`.
    pub const fn timeout_us(&self, pclkb: u32) -> u32 {
        (self.timeout.cycles() as u64 * self.div.divisor() as u64 * 1_000_000 / pclkb as u64)
            as u32
    }
}

/// The IWDT's settings.
pub type IwdtConfig = Config<IwdtDiv>;
/// The WDT's settings.
pub type WdtConfig = Config<WdtDiv>;

/// The registers of one watchdog (the same layout in both): the refresh
/// register, the control, the status and the reset control.
struct Regs {
    base: usize,
}

impl Regs {
    fn start(&self, cr: u16, reset: bool) -> Result<(), StartError> {
        unsafe {
            write_volatile((self.base + 2) as *mut u16, cr);
            write_volatile((self.base + 6) as *mut u8, (reset as u8) << 7);
            // The reserved bits read 0 (RPSS b13-12, RPES b9-8, CKS b7-4,
            // TOPS b1-0). A control that does not read back is fixed
            // already (written before, or OFS0's auto-start).
            if read_volatile((self.base + 2) as *const u16) & 0x33f3 != cr {
                return Err(StartError::Locked);
            }
        }
        self.feed();
        Ok(())
    }

    fn feed(&self) {
        unsafe {
            write_volatile(self.base as *mut u8, 0x00);
            write_volatile(self.base as *mut u8, 0xff);
        }
    }

    fn status(&self) -> u16 {
        unsafe { read_volatile((self.base + 4) as *const u16) }
    }
}

fn cr<D: Copy + Into<u16>>(c: &Config<D>) -> u16 {
    (c.window_start as u16) << 12 | (c.window_end as u16) << 8 | c.div.into() << 4 | c.timeout as u16
}

impl From<IwdtDiv> for u16 {
    fn from(d: IwdtDiv) -> u16 {
        d as u16
    }
}

impl From<WdtDiv> for u16 {
    fn from(d: WdtDiv) -> u16 {
        d as u16
    }
}

macro_rules! watchdog {
    ($name:ident, $periph:ty, $base:expr, $config:ty, $prepare:expr, $what:literal) => {
        #[doc = concat!("The ", $what, " in register start mode; see the [module](self).")]
        pub struct $name {
            _p: $periph,
            regs: Regs,
        }

        impl $name {
            /// Writes the settings and starts the counter (the first feed).
            /// [`StartError::Locked`] if the settings did not take (already
            /// written since the reset, or OFS0's auto-start mode): the
            /// counter then runs as set before, and is not fed.
            pub fn start(p: $periph, config: $config) -> Result<Self, StartError> {
                $prepare;
                let regs = Regs { base: $base };
                regs.start(cr(&config), config.reset)?;
                Ok($name { _p: p, regs })
            }

            /// Starts the count over (refresh register: 00h, then FFh). A
            /// feed outside the window is a refresh error.
            pub fn feed(&mut self) {
                self.regs.feed();
            }

            /// The counter (CNTVAL), counting down to 0; it may be one
            /// count off. Right after `start` it can read 0 for a few cycles
            /// of the timer's clock before the count begins (the RX74M's
            /// IWDT does).
            pub fn counter(&self) -> u16 {
                self.regs.status() & 0x3fff
            }
        }
    };
}

/// Starts the IWDT's own 120 kHz oscillator, which a reset leaves stopped in
/// register start mode (ILOCOCR.ILCSTP = 1; manual 9.2.11), and waits for
/// OSCOVFSR.ILCOVF: 34 LOCO cycles, bounded at a few ms at 240 MHz.
fn start_iwdt_clock() -> Result<(), StartError> {
    const ILOCOCR: *mut u8 = 0x0008_0035 as _;
    const OSCOVFSR: *const u8 = 0x0008_003c as _;
    const PRCR: *mut u16 = 0x0008_03fe as _;
    const ILCOVF: u8 = 1 << 4;
    unsafe {
        if read_volatile(ILOCOCR) & 1 != 0 {
            critical_section::with(|_| {
                let prc = read_volatile(PRCR) & 0x000f;
                write_volatile(PRCR, 0xa500 | prc | 1);
                write_volatile(ILOCOCR, 0);
                write_volatile(PRCR, 0xa500 | prc);
            });
        }
        crate::timeout::wait(1_000_000, || read_volatile(OSCOVFSR) & ILCOVF != 0)
            .map_err(|_| StartError::Clock)
    }
}

watchdog!(Iwdt, pac::Iwdt, 0x0008_8030, IwdtConfig, start_iwdt_clock()?, "independent watchdog timer (IWDT)");
watchdog!(Wdt, pac::Wdt, 0x0008_8020, WdtConfig, (), "watchdog timer (WDT)");
