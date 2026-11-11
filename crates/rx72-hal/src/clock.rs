//! Clock generation: the main oscillator, the PLL and the internal clock
//! dividers (SCKCR), and the system clock source (SCKCR3).
//!
//! After reset the RX72M runs on LOCO (about 240 kHz). [`Config::configure`]
//! starts the main oscillator (a crystal), the PLL from it, sets the
//! dividers and the flash memory wait (MEMWAIT, needed above 120 MHz ICLK)
//! and switches the system clock to the PLL, as in Renesas' r_bsp.

use crate::pac;

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

    /// Switches the system clock to the PLL with this configuration.
    ///
    /// Panics if a clock would exceed its maximum (ICLK 240 MHz, PCLKA and
    /// BCLK 120 MHz, PCLKB / PCLKC / PCLKD / FCLK 60 MHz, PLL 120-240 MHz).
    pub fn configure(&self, system: &pac::System) -> Clocks {
        let c = self.clocks();
        assert!((8_000_000..=24_000_000).contains(&self.main_hz));
        assert!((1..=3).contains(&self.pll_input_div));
        assert!((20..=60).contains(&self.pll_mul_x2));
        assert!((120_000_000..=240_000_000).contains(&c.pll));
        assert!(c.iclk <= 240_000_000 && c.pclka <= 120_000_000 && c.bclk <= 120_000_000);
        assert!(c.pclkb <= 60_000_000 && c.pclkc <= 60_000_000);
        assert!(c.pclkd <= 60_000_000 && c.fclk <= 60_000_000);

        // Lift the write protection (PRC0 and PRC1) of the clock generation
        // registers, MOFCR and MEMWAIT.
        system
            .prcr()
            .write(|w| unsafe { w.prkey().bits(0xa5) }.prc0().set_bit().prc1().set_bit());

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
        while system.oscovfsr().read().moovf().bit_is_clear() {}

        // Flash wait for ICLK above 120 MHz, before the switch.
        if c.iclk > 120_000_000 {
            system.memwait().write(|w| w.memwait().set_bit());
            while system.memwait().read().memwait().bit_is_clear() {}
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
        while system.oscovfsr().read().plovf().bit_is_clear() {}

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

        system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) });
        c
    }
}
