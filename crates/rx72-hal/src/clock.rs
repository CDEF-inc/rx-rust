//! Clock generation: the main oscillator, the PLL and the internal clock
//! dividers (SCKCR), and the system clock source (SCKCR3).
//!
//! After reset the RX72M runs on LOCO (about 240 kHz). [`Config::configure`]
//! starts the main oscillator (a crystal), the PLL from it, sets the
//! dividers and the flash memory wait (MEMWAIT, needed above 120 MHz ICLK)
//! and switches the system clock to the PLL, as in Renesas' r_bsp.

use crate::pac;
use crate::timeout;

/// A divider of SCKCR: the clock is the system clock divided by 2^n.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Div {
    Div1 = 0,
    Div2 = 1,
    Div4 = 2,
    Div8 = 3,
    Div16 = 4,
    Div32 = 5,
    Div64 = 6,
}

impl Div {
    const fn apply(self, hz: u32) -> u32 {
        hz >> self as u32
    }
}

/// Clock configuration from the main oscillator through the PLL.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Frequency of the main oscillator (crystal), 8 to 24 MHz.
    pub main_hz: u32,
    /// PLL input divider: 1, 2 or 3.
    pub pll_input_div: u8,
    /// PLL multiplication times 2 (20 = x10.0); 20 to 60.
    pub pll_mul_x2: u8,
    pub iclk: Div,
    pub pclka: Div,
    pub pclkb: Div,
    pub pclkc: Div,
    pub pclkd: Div,
    pub fclk: Div,
    pub bclk: Div,
}

/// The frequencies of the internal clocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clocks {
    pub pll: u32,
    pub iclk: u32,
    pub pclka: u32,
    pub pclkb: u32,
    pub pclkc: u32,
    pub pclkd: u32,
    pub fclk: u32,
    pub bclk: u32,
}

impl Config {
    /// The maximum speeds from a 24 MHz crystal (the AP-RX72M-0A's): PLL
    /// x10 = 240 MHz; ICLK 240 MHz, PCLKA and BCLK 120 MHz, PCLKB, PCLKC,
    /// PCLKD and FCLK 60 MHz.
    pub const fn pll_240mhz_from_24mhz() -> Self {
        Config {
            main_hz: 24_000_000,
            pll_input_div: 1,
            pll_mul_x2: 20,
            iclk: Div::Div1,
            pclka: Div::Div2,
            pclkb: Div::Div4,
            pclkc: Div::Div4,
            pclkd: Div::Div4,
            fclk: Div::Div4,
            bclk: Div::Div2,
        }
    }

    /// The frequencies this configuration gives.
    pub const fn clocks(&self) -> Clocks {
        let pll = self.main_hz / self.pll_input_div as u32 * self.pll_mul_x2 as u32 / 2;
        Clocks {
            pll,
            iclk: self.iclk.apply(pll),
            pclka: self.pclka.apply(pll),
            pclkb: self.pclkb.apply(pll),
            pclkc: self.pclkc.apply(pll),
            pclkd: self.pclkd.apply(pll),
            fclk: self.fclk.apply(pll),
            bclk: self.bclk.apply(pll),
        }
    }

    /// Whether the settings are within the chip's limits.
    fn valid(&self) -> bool {
        let c = self.clocks();
        (8_000_000..=24_000_000).contains(&self.main_hz)
            && (1..=3).contains(&self.pll_input_div)
            && (20..=60).contains(&self.pll_mul_x2)
            && (120_000_000..=240_000_000).contains(&c.pll)
            && c.iclk <= 240_000_000
            && c.pclka <= 120_000_000
            && c.bclk <= 120_000_000
            && c.pclkb <= 60_000_000
            && c.pclkc <= 60_000_000
            && c.pclkd <= 60_000_000
            && c.fclk <= 60_000_000
    }

    /// Switches the system clock to the PLL with this configuration.
    ///
    /// Panics where [`try_configure`](Self::try_configure) returns an
    /// error: a setting out of range, or a crystal or PLL that does not
    /// start.
    pub fn configure(&self, system: &pac::System) -> Clocks {
        self.try_configure(system).expect("clock set-up failed")
    }

    /// Switches the system clock to the PLL with this configuration.
    ///
    /// - [`ClockError::OutOfRange`] if a clock would exceed its maximum
    ///   (ICLK 240 MHz, PCLKA and BCLK 120 MHz, PCLKB / PCLKC / PCLKD /
    ///   FCLK 60 MHz, PLL 120-240 MHz) or a setting is outside its range;
    ///   nothing is changed.
    /// - [`ClockError::MainOscillator`] / [`ClockError::Pll`] if the main
    ///   oscillator does not become stable (OSCOVFSR.MOOVF), or the PLL does
    ///   not lock (PLOVF), within 100 ms: a missing or faulty crystal. The
    ///   system clock stays as it was (LOCO after a reset); the oscillator
    ///   or PLL is left started.
    pub fn try_configure(&self, system: &pac::System) -> Result<Clocks, ClockError> {
        if !self.valid() {
            return Err(ClockError::OutOfRange);
        }
        let c = self.clocks();
        // The waits below run on the current system clock (LOCO after a
        // reset): bound them at its fastest.
        let spins_100ms = timeout::spins(current_iclk_max(system), 100_000);

        // Lift the write protection (PRC0 and PRC1) of the clock generation
        // registers, MOFCR and MEMWAIT.
        system
            .prcr()
            .write(|w| unsafe { w.prkey().bits(0xa5) }.prc0().set_bit().prc1().set_bit());
        let r = self.start(system, &c, spins_100ms);
        system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) });
        r.map(|()| c)
    }

    /// The set-up itself, with the protection lifted.
    fn start(&self, system: &pac::System, c: &Clocks, spins_100ms: u32) -> Result<(), ClockError> {

        // Main oscillator: a crystal, drive for its frequency.
        let modrv2 = match self.main_hz {
            20_000_001.. => 0b00,
            16_000_001.. => 0b01,
            8_000_001.. => 0b10,
            _ => 0b11,
        };
        system
            .mofcr()
            .write(|w| unsafe { w.modrv2().bits(modrv2) }.mosel().clear_bit());
        system.moscwtcr().write(|w| unsafe { w.msts().bits(0x53) });
        system.mosccr().write(|w| w.mostp().clear_bit());
        timeout::wait(spins_100ms, || system.oscovfsr().read().moovf().bit_is_set())
            .map_err(|_| ClockError::MainOscillator)?;

        // Flash wait for ICLK above 120 MHz, before the switch; read back
        // as r_bsp does (it reads back at once).
        if c.iclk > 120_000_000 {
            system.memwait().write(|w| w.memwait().set_bit());
            let _ = timeout::wait(spins_100ms, || system.memwait().read().memwait().bit_is_set());
        }

        // PLL from the main oscillator: STC = multiplication x 2 - 1.
        system.pllcr().write(|w| unsafe {
            w.stc()
                .bits(self.pll_mul_x2 - 1)
                .pllsrcsel()
                .clear_bit()
                .plidiv()
                .bits(self.pll_input_div - 1)
        });
        system.pllcr2().write(|w| w.pllen().clear_bit());
        timeout::wait(spins_100ms, || system.oscovfsr().read().plovf().bit_is_set())
            .map_err(|_| ClockError::Pll)?;

        // Dividers (keep PSTOP0 / PSTOP1, the BCLK / SDCLK pin outputs),
        // then the PLL as the system clock (CKSEL = 100b).
        system.sckcr().modify(|_, w| unsafe {
            w.fck()
                .bits(self.fclk as u8)
                .ick()
                .bits(self.iclk as u8)
                .bck()
                .bits(self.bclk as u8)
                .pcka()
                .bits(self.pclka as u8)
                .pckb()
                .bits(self.pclkb as u8)
                .pckc()
                .bits(self.pclkc as u8)
                .pckd()
                .bits(self.pclkd as u8)
        });
        system.sckcr3().write(|w| unsafe { w.cksel().bits(0b100) });
        Ok(())
    }
}

/// Why [`Config::try_configure`] failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClockError {
    /// A setting is out of range; nothing was changed.
    OutOfRange,
    /// The main oscillator did not become stable within 100 ms.
    MainOscillator,
    /// The PLL did not lock within 100 ms.
    Pll,
}

impl core::fmt::Display for ClockError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            ClockError::OutOfRange => "clock setting out of range",
            ClockError::MainOscillator => "main oscillator not stable",
            ClockError::Pll => "PLL not locked",
        })
    }
}

impl core::error::Error for ClockError {}

/// The fastest the current ICLK can be, from the source in SCKCR3 and the
/// divider in SCKCR: LOCO 264 kHz (240 kHz + 10%), HOCO 20 MHz, the main
/// oscillator 24 MHz, the sub-clock 32.768 kHz, the PLL 240 MHz.
fn current_iclk_max(system: &pac::System) -> u32 {
    let source = match system.sckcr3().read().cksel().bits() {
        0b000 => 264_000,
        0b001 => 20_000_000,
        0b010 => 24_000_000,
        0b011 => 32_768,
        _ => 240_000_000,
    };
    source >> system.sckcr().read().ick().bits()
}
