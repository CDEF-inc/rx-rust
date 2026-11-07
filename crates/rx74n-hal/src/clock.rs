//! Clock generation: the main clock oscillator, PLL1 and the internal clock
//! dividers (SCKDIVCR / SCKDIVCR2), and the system clock source (SCKSCR).
//!
//! A reset selects the 8 MHz MOCO for the system clock and for SCICLK, with
//! every divider at 1 ([`Clocks::after_reset`]). [`Config::configure`]
//! starts the main clock, PLL1 from it, sets the MRAM and SRAM wait cycles
//! (MRCFREQ / MREFREQ, SRAMWTSC) and, for 600 MHz, the VDD target (VSCR),
//! then the dividers, and switches the system clock to PLL1P, following
//! the hardware manual's procedure (Table 9.7, Figure 65.14). SCICLK stays
//! on the MOCO.

use core::ptr::read_volatile;

use crate::pac;

/// A divider of SCKDIVCR / SCKDIVCR2 (the values are the register's).
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
    Div3 = 8,
    Div6 = 9,
    Div12 = 10,
    Div24 = 11,
}

impl Div {
    pub const fn divisor(self) -> u32 {
        match self {
            Div::Div3 => 3,
            Div::Div6 => 6,
            Div::Div12 => 12,
            Div::Div24 => 24,
            d => 1 << d as u32,
        }
    }

    const fn apply(self, hz: u32) -> u32 {
        hz / self.divisor()
    }
}

/// The VDD target of the voltage scaling (VSCR.VSCM). A reset selects
/// VSCR_2; CPUCLK0 above 480 MHz needs VSCR_1 (in DCDC mode, Table 67.52).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voltage {
    Vscr1,
    Vscr2,
}

/// Clock configuration from the main clock through PLL1 (its output P is
/// the system clock source).
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Frequency of the main clock, 8 to 48 MHz.
    pub main_hz: u32,
    /// The main clock comes from an external clock on EXTAL, not from a
    /// crystal across EXTAL and XTAL (as on the EK-RX74N).
    pub main_external: bool,
    /// PLL1 input divider: 1, 2 or 3 (the input must be 8 to 24 MHz).
    pub pll_input_div: u8,
    /// PLL1 multiplication, 40 to 300 (the VCO must be 960 to 2400 MHz).
    pub pll_mul: u16,
    /// PLL1 output P divider: 2, 3, 4, 6, 8 or 16.
    pub pll_p_div: u8,
    pub cpuclk: Div,
    pub mriclk: Div,
    pub iclk: Div,
    pub mrpclk: Div,
    pub pclka: Div,
    /// PCLKB, and PCLKC (which must be the same).
    pub pclkb: Div,
    pub pclkd: Div,
    pub pclke: Div,
    pub bclk: Div,
    pub voltage: Voltage,
}

/// The frequencies of the internal clocks in Hz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clocks {
    /// PLL1 output P (0 when the PLL is not used).
    pub pll1p: u32,
    pub cpuclk: u32,
    pub mriclk: u32,
    pub iclk: u32,
    pub mrpclk: u32,
    pub pclka: u32,
    pub pclkb: u32,
    pub pclkc: u32,
    pub pclkd: u32,
    pub pclke: u32,
    pub bclk: u32,
    /// The RSCI's operating clock (TCLK in the hardware manual).
    pub sciclk: u32,
}

impl Clocks {
    /// The clocks after a reset.
    pub const fn after_reset() -> Self {
        const MOCO: u32 = 8_000_000;
        Clocks {
            pll1p: 0,
            cpuclk: MOCO,
            mriclk: MOCO,
            iclk: MOCO,
            mrpclk: MOCO,
            pclka: MOCO,
            pclkb: MOCO,
            pclkc: MOCO,
            pclkd: MOCO,
            pclke: MOCO,
            bclk: MOCO,
            sciclk: MOCO,
        }
    }
}

impl Config {
    /// CPUCLK0 480 MHz from the EK-RX74N's 24 MHz crystal, within the
    /// reset's VSCR_2 (frequency setting 2 of Table 67.8): PLL1 x40 (VCO
    /// 960 MHz) / 2; ICLK, MRICLK and PCLKE 240 MHz, MRPCLK, PCLKA, PCLKD
    /// and BCLK 120 MHz, PCLKB and PCLKC 60 MHz.
    pub const fn pll_480mhz_from_24mhz() -> Self {
        Config {
            main_hz: 24_000_000,
            main_external: false,
            pll_input_div: 1,
            pll_mul: 40,
            pll_p_div: 2,
            cpuclk: Div::Div1,
            mriclk: Div::Div2,
            iclk: Div::Div2,
            mrpclk: Div::Div4,
            pclka: Div::Div4,
            pclkb: Div::Div8,
            pclkd: Div::Div4,
            pclke: Div::Div2,
            bclk: Div::Div4,
            voltage: Voltage::Vscr2,
        }
    }

    /// CPUCLK0 600 MHz from the EK-RX74N's 24 MHz crystal, the maximum
    /// (frequency setting 1 of Table 67.8). It needs VSCR_1 in DCDC mode,
    /// in which the board starts (UMRECR.DCDCCTL = 1). PLL1 / 3 x150 (VCO
    /// 1200 MHz) / 2, as Renesas' board configuration for the EK-RX74N;
    /// ICLK, MRICLK and PCLKE 300 MHz, MRPCLK, PCLKA, PCLKD and BCLK 150
    /// MHz, PCLKB and PCLKC 75 MHz.
    pub const fn pll_600mhz_from_24mhz() -> Self {
        Config {
            pll_input_div: 3,
            pll_mul: 150,
            voltage: Voltage::Vscr1,
            ..Self::pll_480mhz_from_24mhz()
        }
    }

    /// The frequencies this configuration gives.
    pub const fn clocks(&self) -> Clocks {
        let pll1p =
            self.main_hz / self.pll_input_div as u32 * self.pll_mul as u32 / self.pll_p_div as u32;
        Clocks {
            pll1p,
            cpuclk: self.cpuclk.apply(pll1p),
            mriclk: self.mriclk.apply(pll1p),
            iclk: self.iclk.apply(pll1p),
            mrpclk: self.mrpclk.apply(pll1p),
            pclka: self.pclka.apply(pll1p),
            pclkb: self.pclkb.apply(pll1p),
            pclkc: self.pclkb.apply(pll1p),
            pclkd: self.pclkd.apply(pll1p),
            pclke: self.pclke.apply(pll1p),
            bclk: self.bclk.apply(pll1p),
            sciclk: Clocks::after_reset().sciclk,
        }
    }

    /// Panics unless the configuration is within the hardware manual's
    /// limits for the R5K574NxB4xx (Tables 9.1, 9.2 and 67.52).
    fn check(&self, c: &Clocks) {
        const MHZ: u32 = 1_000_000;
        assert!((8 * MHZ..=48 * MHZ).contains(&self.main_hz));
        assert!((1..=3).contains(&self.pll_input_div));
        assert!((8 * MHZ..=24 * MHZ).contains(&(self.main_hz / self.pll_input_div as u32)));
        assert!((40..=300).contains(&self.pll_mul));
        let vco = self.main_hz as u64 / self.pll_input_div as u64 * self.pll_mul as u64;
        assert!((960 * MHZ as u64..=2400 * MHZ as u64).contains(&vco));
        assert!(matches!(self.pll_p_div, 2 | 3 | 4 | 6 | 8 | 16));
        assert!((60 * MHZ..=600 * MHZ).contains(&c.pll1p));

        let cpu_max = match self.voltage {
            Voltage::Vscr1 => 600 * MHZ,
            Voltage::Vscr2 => 480 * MHZ,
        };
        assert!(c.cpuclk <= cpu_max && c.mriclk <= 300 * MHZ && c.iclk <= 300 * MHZ);
        assert!(c.mrpclk <= 150 * MHZ && c.pclka <= 150 * MHZ && c.pclkb <= 75 * MHZ);
        assert!(c.pclkd <= 150 * MHZ && c.pclke <= 300 * MHZ && c.bclk <= 150 * MHZ);

        // CPUCLK0 : MRICLK : ICLK, ICLK : MRPCLK, PCLKA, PCLKB and BCLK
        // are N : 1; ICLK : PCLKD and PCLKE are N : 1 or 1 : N.
        let n_to_1 = |fast: Div, slow: Div| slow.divisor() % fast.divisor() == 0;
        let either = |a: Div, b: Div| n_to_1(a, b) || n_to_1(b, a);
        assert!(n_to_1(self.cpuclk, self.mriclk) && n_to_1(self.mriclk, self.iclk));
        assert!(n_to_1(self.iclk, self.mrpclk) && n_to_1(self.iclk, self.bclk));
        assert!(n_to_1(self.iclk, self.pclka) && n_to_1(self.pclka, self.pclkb));
        assert!(either(self.iclk, self.pclkd) && either(self.iclk, self.pclke));

        // No division by 3, 6, 12 or 24 together with 2, 4, ... 64.
        let divs = [
            self.cpuclk,
            self.mriclk,
            self.iclk,
            self.mrpclk,
            self.pclka,
            self.pclkb,
            self.pclkd,
            self.pclke,
            self.bclk,
        ];
        let by3 = divs.iter().any(|d| d.divisor() % 3 == 0);
        let by2 = divs.iter().any(|d| d.divisor() % 3 != 0 && d.divisor() > 1);
        assert!(!(by3 && by2));
    }

    /// Switches the system clock to PLL1P with this configuration. Call it
    /// once, after a reset (the MOCO is the system clock and PLL1 is
    /// stopped).
    ///
    /// Panics if the configuration is outside the limits, or if the clocks
    /// are no longer as a reset leaves them.
    pub fn configure(&self, sysc: &pac::Sysc, sram: &pac::Sram, mram: &pac::Mram) -> Clocks {
        let c = self.clocks();
        self.check(&c);
        assert!(sysc.sckscr().read().cksel().bits() == 0b001);
        assert!(sysc.pllcr().read().pllstp().bit_is_set());

        // Lift the write protection of the clock generation registers
        // (PRC0) and of PLL1LDOCR and VSCR (PRC1).
        let prc = sysc.prcr_s().read().bits() & 0x00ff;
        sysc.prcr_s()
            .write(|w| unsafe { w.prkey().bits(0xa5) }.prc0().set_bit().prc1().set_bit());

        // PLL1's LDO, enabled after a reset; turned on it needs 25 us (400
        // cycles is 50 us at the MOCO's 8 MHz).
        if sysc.pll1ldocr().read().ldostp().bit_is_set() {
            sysc.pll1ldocr().modify(|_, w| w.ldostp().clear_bit());
            spin(400);
        }

        // The main clock. MOMCR and MOSCWTCR are written while it is
        // stopped. A crystal gets the reset's stabilization wait (MSTS =
        // 0101b, about 2.1 ms); an external clock needs none. OSCSF's
        // MOSCSF only says that the wait is over, not that it oscillates.
        sysc.momcr()
            .modify(|_, w| unsafe { w.mosel().bit(self.main_external).modrv0().bits(0b101) });
        let msts = if self.main_external { 0b0000 } else { 0b0101 };
        sysc.moscwtcr().write(|w| unsafe { w.msts().bits(msts) });
        sysc.mosccr().write(|w| w.mostp().clear_bit());
        while sysc.mosccr().read().mostp().bit_is_set() {}
        while sysc.oscsf().read().moscsf().bit_is_clear() {}

        // PLL1 from the main clock. Outputs Q and R keep the reset's
        // division by 6, within their 106.6-500 MHz for any allowed VCO.
        sysc.pllccr().modify(|_, w| unsafe {
            w.pllmul()
                .bits(self.pll_mul - 1)
                .pllmulnf()
                .bits(0)
                .plsrcsel()
                .clear_bit()
                .plidiv()
                .bits(self.pll_input_div - 1)
        });
        sysc.pllccr2().modify(|_, w| unsafe { w.plodivp().bits(self.pll_p_div - 1) });
        sysc.pllcr().write(|w| w.pllstp().clear_bit());
        while sysc.oscsf().read().pllsf().bit_is_clear() {}

        // The MRAM wait cycles follow from the frequencies in MHz, rounded
        // up; they are set before the clocks go up, each read back.
        let mrc = c.mriclk.div_ceil(1_000_000) as u16;
        mram.mrcfreq().write(|w| unsafe { w.key().bits(0x1e).mrcmhz().bits(mrc) });
        while mram.mrcfreq().read().mrcmhz().bits() != mrc {}
        let mre = c.mrpclk.div_ceil(1_000_000) as u8;
        mram.mrefreq().write(|w| unsafe { w.key().bits(0xe1).mremhz().bits(mre) });
        while mram.mrefreq().read().mremhz().bits() != mre {}
        // One SRAM wait cycle above 150 MHz ICLK.
        if c.iclk > 150_000_000 {
            sram.sramprcr_s().write(|w| unsafe { w.kw().bits(0xa5) }.pr().set_bit());
            sram.sramwtsc().write(|w| w.wten().set_bit());
            sram.sramprcr_s().write(|w| unsafe { w.kw().bits(0xa5) });
        }

        if self.voltage == Voltage::Vscr1 {
            sysc.vscr().write(|w| unsafe { w.vscm().bits(0b001) });
            while sysc.vscr().read().vscmtsf().bit_is_set() {}
        }

        // The dividers: SCKDIVCR first, so that CPUCLK0 and MRICLK never
        // fall below ICLK in between. PCLKC is PCLKB's.
        sysc.sckdivcr().write(|w| unsafe {
            w.mrpck()
                .bits(self.mrpclk as u8)
                .ick()
                .bits(self.iclk as u8)
                .pcke()
                .bits(self.pclke as u8)
                .bck()
                .bits(self.bclk as u8)
                .pcka()
                .bits(self.pclka as u8)
                .pckb()
                .bits(self.pclkb as u8)
                .pckc()
                .bits(self.pclkb as u8)
                .pckd()
                .bits(self.pclkd as u8)
        });
        sysc.sckdivcr2()
            .write(|w| unsafe { w.mrick().bits(self.mriclk as u8).cpuck0().bits(self.cpuclk as u8) });

        // PLL1P as the system clock, then the wait of Figure 9.14 (30 us in
        // DCDC mode, 10 us with an external VDD), at the new CPUCLK0.
        sysc.sckscr().write(|w| unsafe { w.cksel().bits(0b101) });
        spin(c.cpuclk / 1_000_000 * 30);

        sysc.prcr_s().write(|w| unsafe { w.bits(0xa500 | prc) });
        c
    }
}

/// At least `n` CPU cycles: each iteration is a volatile load and a branch.
fn spin(n: u32) {
    let x = 0u8;
    for _ in 0..n {
        unsafe { read_volatile(&x) };
    }
}
