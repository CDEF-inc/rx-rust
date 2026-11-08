//! The 16-bit A/D converter (ADC16H) in SAR mode, unit 0 only, one
//! conversion at a time: AN000-AN007 (P400-P407), the self-diagnosis
//! voltages, the internal reference voltage, the D/A converters' outputs
//! and, with [`TemperatureSensor`], the temperature sensor.
//!
//! ADC16H is not the RX72M's S12AD:
//! - Its conversion clock ADCLK must be 25-60 MHz, and PCLKA (120 MHz) may
//!   not be its source (manual table 67.52, note 2). [`Adc::adc16h`] sets
//!   ADCCLK in the clock generation circuit (ADCCKCR / ADCCKDIVCR, section
//!   9) to PLL1P divided by the smallest of 1, 2, 4, 6, 8, 10, 16, 32 that
//!   gives at most 60 MHz (480 MHz: 60 MHz with /8; 600 MHz: /10), and
//!   uses it undivided (ADCLKCR = 0).
//! - It must be calibrated before converting (table 60.18). The constructor
//!   puts the unused units 1 and 2 to sleep (ADUSLPCR1 / 2: a sleeping unit
//!   is not calibrated) and calibrates unit 0 (internal circuit, gain and
//!   offset: ADCALSTR = 3), waiting for ADCALENDSR.
//! - A conversion goes through a table: analog channel -> virtual channel
//!   (ADCHCRn) -> scan group -> unit. Each read sets virtual channel 0 of
//!   scan group 0 (unit 0, SAR single scan, the reset settings) to the
//!   source, starts it by software (ADSTR0) and polls ADSCANENDSR.
//! - SAR results are 12 bits (more is not guaranteed in SAR mode): the
//!   driver formats them as 12-bit unsigned (ADDOPCRC.ADPRC = 10b,
//!   SIGNSEL = 1, which SAR mode needs except for the self-diagnosis). The
//!   self-diagnosis converts differentially, 16-bit signed (table 60.21).
//!
//! The reset values of the sampling time (2 states), the successive
//! conversion time (ADCNVSTR, 3 states) and SIGNSEL are not usable in SAR
//! mode; the constructor sets the conversion time to 100 ns or more (table
//! 67.116: 100-200 ns), sampling table 0 (the AN pins and the
//! self-diagnosis) to 1 us and table 1 (the internal sources: at least
//! 4.15 us, the D/A outputs 2 us) to 5 us. [`Adc::set_sampling_states`]
//! changes table 0, for example for a source of higher impedance.
//!
//! Registers at their secure addresses (0x87438000), addressed directly.

use core::ptr::{read_volatile, write_volatile};

use crate::clock::Clocks;
use crate::gpio::{Analog, Pin};
use crate::mstp::{self, Mstpcr};
use crate::pac;

const BASE: usize = 0x8743_8000;
const ADCLKENR: usize = BASE;
const ADCLKSR: usize = BASE + 0x004;
const ADCLKCR: usize = BASE + 0x008;
const ADUSLPCR1: usize = BASE + 0x014;
const ADUSLPCR2: usize = BASE + 0x018;
const ADSGER: usize = BASE + 0x048;
const ADSGDCR0: usize = BASE + 0x200;
const ADSSTR0: usize = BASE + 0x240;
const ADCNVSTR: usize = BASE + 0x260;
const ADCALSTCR: usize = BASE + 0x264;
const ADREFCR: usize = BASE + 0x320;
const ADCHCR0: usize = BASE + 0x600;
const ADDOPCRC0: usize = BASE + 0x60c;
const ADCALSTR: usize = BASE + 0xc00;
const ADSTR0: usize = BASE + 0xc20;
const ADERSR: usize = BASE + 0xc88;
const ADERSCR: usize = BASE + 0xc8c;
const ADCALENDSR: usize = BASE + 0xc98;
const ADCALENDSCR: usize = BASE + 0xc9c;
const ADSCANENDSR: usize = BASE + 0xd50;
const ADSCANENDSCR: usize = BASE + 0xd54;
const ADDR0: usize = 0x8743_a000;
const ADEXDR0: usize = 0x8743_a180;

// The ADCCLK selection in the clock generation circuit, behind PRCR_S.PRC0.
const ADCCKDIVCR: *mut u8 = 0x8701_905a_usize as _;
const ADCCKCR: *mut u8 = 0x8701_905b_usize as _;
const ADCCKSREQ: u8 = 1 << 6;
const ADCCKSRDY: u8 = 1 << 7;
const ADCCKSEL_PLL1P: u8 = 0b0101;
const PRCR_S: *mut u16 = 0x8701_93fa_usize as _;

// TEMPS
const TSCR: *mut u8 = 0x8783_5000_usize as _;
const TSCDR: *const u32 = 0x86c1_2da0_usize as _;
const TSCDR2: *const u32 = 0x86c1_2da4_usize as _;
const TSEN: u8 = 1 << 7;
const TSOE: u8 = 1 << 4;

/// ADCCKDIVCR.ADCCKDIV codes, smallest divider first.
const ADCCK_DIVS: [(u32, u8); 8] = [
    (1, 0b0000),
    (2, 0b0001),
    (4, 0b0010),
    (6, 0b0011),
    (8, 0b0100),
    (10, 0b0111),
    (16, 0b1000),
    (32, 0b1001),
];

// ADCHCRn.CNVCS of the internal sources (table 60.5) and their result
// registers (ADEXDRn, n = CNVCS - 60h).
const CNVCS_SELF_DIAG: u32 = 0x60;
const CNVCS_TEMPERATURE: u32 = 0x64;
const CNVCS_REFERENCE: u32 = 0x65;
const CNVCS_DA0: u32 = 0x68;

// ADDOPCRCn: 12-bit unsigned; 16-bit signed (the self-diagnosis).
const FORMAT_12_UNSIGNED: u32 = 1 << 20 | 0b10 << 16;
const FORMAT_16_SIGNED: u32 = 0;

/// A conversion result marked invalid (ERR, bit 31 of the data register).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConversionError;

/// The voltage of a self-diagnosis (ADSGDCRn.DIAGVAL), with its ideal
/// 16-bit signed result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum DiagVoltage {
    /// 0000h
    Zero = 0b100,
    /// 8000h (the most negative)
    Negative = 0b101,
    /// 7FFFh (the most positive)
    Positive = 0b110,
}

/// An analog pin of unit 0 and its channel number (`n` of AN00n).
pub trait Channel {
    const CHANNEL: u8;
}

macro_rules! channels {
    ($($bit:literal),*) => {
        $(impl Channel for Pin<0x04, $bit, Analog> {
            const CHANNEL: u8 = $bit;
        })*
    };
}

channels!(0, 1, 2, 3, 4, 5, 6, 7);

fn rd32(a: usize) -> u32 {
    unsafe { read_volatile(a as *const u32) }
}

fn wr32(a: usize, v: u32) {
    unsafe { write_volatile(a as *mut u32, v) }
}

/// Waits at least `us` microseconds: a loop of at least one cycle of the
/// CPU's clock CPUCLK0 per iteration (not ICLK, which may be slower).
fn spin_us(cpuclk: u32, us: u32) {
    spin(cpuclk.div_ceil(1_000_000) * us);
}

/// A loop of `n` iterations, each at least one CPUCLK0 cycle. core::hint::spin_loop is nothing on RX, so a loop of only that is
/// removed by the optimizer; the volatile read keeps each iteration.
fn spin(n: u32) {
    let keep = 0u8;
    for _ in 0..n {
        unsafe { read_volatile(&keep) };
    }
}

/// ADC16H unit 0.
pub struct Adc {
    _adc: pac::AdcB,
    adclk: u32,
    /// CPUCLK0 in Hz, for the waits.
    cpuclk: u32,
    /// CPUCLK0 cycles for 6 PCLKA cycles, the gap after a scan.
    scan_gap: u32,
    /// Sampling states of table 0 (the AN pins, the self-diagnosis) and
    /// table 1 (the internal sources).
    sst0: u32,
    sst1: u32,
}

impl Adc {
    /// Sets ADCCLK up from PLL1P, takes ADC16H out of module stop
    /// (MSTPCRD.MSTPD21), puts units 1 and 2 to sleep, starts ADCLK and
    /// calibrates unit 0.
    ///
    /// Panics if PLL1 is not running, if no divider of PLL1P gives 25-60
    /// MHz, or if the calibration reports an error (ADERSR.ADERF0).
    pub fn adc16h(adc: pac::AdcB, clocks: &Clocks) -> Self {
        let &(div, code) = ADCCK_DIVS
            .iter()
            .find(|&&(d, _)| clocks.pll1p / d <= 60_000_000)
            .expect("PLL1 not running");
        let adclk = clocks.pll1p / div;
        assert!(adclk >= 25_000_000, "no ADCLK of 25-60 MHz from PLL1P");
        set_adcclk(code);
        mstp::start(Mstpcr::D, 21);
        wr32(ADUSLPCR1, 1);
        wr32(ADUSLPCR2, 1);
        // ADCLK = ADCCLK / 1, then enabled.
        wr32(ADCLKCR, 0);
        wr32(ADCLKENR, 1);
        while rd32(ADCLKSR) & 1 == 0 {}

        // Successive conversion time of 100 ns or more, the same in all
        // units (ADCNVSTR.CST0-2), and for the calibration.
        let cst = (adclk as u64 * 100).div_ceil(1_000_000_000).max(3) as u32;
        wr32(ADCNVSTR, cst << 16 | cst << 8 | cst);
        let mut adc = Adc {
            _adc: adc,
            adclk,
            cpuclk: clocks.cpuclk,
            scan_gap: 6 * clocks.cpuclk.div_ceil(clocks.pclka),
            sst0: 0,
            sst1: adclk.div_ceil(1_000_000) * 5,
        };
        adc.set_sampling_states(adclk.div_ceil(1_000_000) as u16);
        // Scan group 0 on unit 0 (ADSGCR0 and ADMDR keep their reset
        // values: unit 0, SAR single scan).
        wr32(ADSGER, 1);
        // A/D start-up time, 2 us.
        spin_us(clocks.cpuclk, 2);

        // Calibration: internal circuit, then gain and offset.
        wr32(ADCALSTCR, cst << 16 | adc.sst0);
        wr32(ADERSCR, 1);
        wr32(ADCALSTR, 0b11);
        while rd32(ADCALENDSR) & 1 == 0 {}
        wr32(ADCALENDSCR, 1);
        assert!(rd32(ADERSR) & 1 == 0, "A/D calibration failed");
        adc
    }

    /// The A/D conversion clock in Hz.
    pub fn adclk(&self) -> u32 {
        self.adclk
    }

    /// Sets sampling table 0 (the AN pins and the self-diagnosis) to
    /// `states` ADCLK cycles (ADSSTR0, bits 9-0).
    ///
    /// Panics if `states` is below 1 ADCLK cycle + 40 ns (table 67.116) or
    /// above 1023.
    pub fn set_sampling_states(&mut self, states: u16) {
        let min = 1 + (self.adclk as u64 * 40).div_ceil(1_000_000_000) as u16;
        assert!((min..=1023).contains(&states));
        self.sst0 = states as u32;
        wr32(ADSSTR0, self.sst1 << 16 | self.sst0);
    }

    /// The sampling states of table 0.
    pub fn sampling_states(&self) -> u16 {
        self.sst0 as u16
    }

    /// Converts `cnvcs` (differentially if `differential`) with sampling
    /// table `sst` and data format `format`, and returns `result`.
    fn convert(
        &mut self,
        cnvcs: u32,
        differential: bool,
        sst: u32,
        format: u32,
        result: usize,
    ) -> u32 {
        wr32(ADCHCR0, 1 | cnvcs << 8 | (differential as u32) << 15 | sst << 16);
        wr32(ADDOPCRC0, format);
        wr32(ADSTR0, 1);
        while rd32(ADSCANENDSR) & 1 == 0 {}
        wr32(ADSCANENDSCR, 1);
        // The next start comes at least 6 PCLKA cycles after the end.
        spin(self.scan_gap);
        rd32(result)
    }

    /// Converts the input of `pin` and returns the 12-bit result.
    pub fn read<PIN: Channel>(&mut self, _pin: &PIN) -> Result<u16, ConversionError> {
        let ch = PIN::CHANNEL as usize;
        let r = self.convert(ch as u32, false, 0, FORMAT_12_UNSIGNED, ADDR0 + 4 * ch);
        data(r)
    }

    /// Converts the self-diagnosis voltage `v` (differential, 16-bit
    /// signed; ADSGDCR0.DIAGVAL, reset to 0 afterwards). The result of
    /// [`DiagVoltage::Zero`] is ideally 0, [`DiagVoltage::Negative`] 8000h
    /// (-32768), [`DiagVoltage::Positive`] 7FFFh.
    pub fn self_diagnosis(&mut self, v: DiagVoltage) -> Result<i16, ConversionError> {
        wr32(ADSGDCR0, v as u32);
        let r = self.convert(CNVCS_SELF_DIAG, true, 0, FORMAT_16_SIGNED, ADEXDR0);
        wr32(ADSGDCR0, 0);
        data(r).map(|d| d as i16)
    }

    /// Converts the internal reference voltage (ADREFCR.VDE, 0.77-0.84 V),
    /// with the 5 us sampling of table 1.
    pub fn read_internal_reference(&mut self) -> Result<u16, ConversionError> {
        wr32(ADREFCR, 1);
        // No settling time is given; wait as long as the sampling.
        spin_us(self.cpuclk, 5);
        let r = self.convert(CNVCS_REFERENCE, false, 1, FORMAT_12_UNSIGNED, ADEXDR0 + 4 * 5);
        wr32(ADREFCR, 0);
        data(r)
    }

    /// Converts the internal output of D/A converter `n` (0 or 1; the DAC
    /// must be running with DACR0.DAOUTDIS = 1), with the 5 us sampling of
    /// table 1 (at least 2 us).
    ///
    /// Panics if `n` is not 0 or 1.
    pub fn read_da(&mut self, n: u8) -> Result<u16, ConversionError> {
        assert!(n < 2);
        let cnvcs = CNVCS_DA0 + n as u32;
        let r = self.convert(cnvcs, false, 1, FORMAT_12_UNSIGNED, ADEXDR0 + 4 * (8 + n as usize));
        data(r)
    }
}

/// The data of a result register (bits 15-0), or the error of ERR (bit 31).
fn data(r: u32) -> Result<u16, ConversionError> {
    if r & 1 << 31 != 0 { Err(ConversionError) } else { Ok(r as u16) }
}

/// Selects PLL1P / the divider `code` as ADCCLK (manual 9.2.x: ADCCKSREQ,
/// wait for ADCCKSRDY, write the divider and the source, ADCCKSREQ off,
/// wait for ADCCKSRDY to clear), with PRCR_S.PRC0 set.
fn set_adcclk(code: u8) {
    critical_section::with(|_| unsafe {
        let prc = read_volatile(PRCR_S) & 0x00ff;
        write_volatile(PRCR_S, 0xa500 | prc | 1);
        let sel = read_volatile(ADCCKCR) & 0x0f;
        write_volatile(ADCCKCR, sel | ADCCKSREQ);
        while read_volatile(ADCCKCR) & ADCCKSRDY == 0 {}
        write_volatile(ADCCKDIVCR, code);
        write_volatile(ADCCKCR, ADCCKSEL_PLL1P | ADCCKSREQ);
        write_volatile(ADCCKCR, ADCCKSEL_PLL1P);
        while read_volatile(ADCCKCR) & ADCCKSRDY != 0 {}
        write_volatile(PRCR_S, 0xa500 | prc);
    });
}

/// The temperature sensor (TEMPS), converted by ADC16H.
pub struct TemperatureSensor {
    _temps: pac::Temps,
}

impl TemperatureSensor {
    /// Takes the sensor out of module stop (MSTPCRD.MSTPD22); it stays off
    /// between reads.
    pub fn new(temps: pac::Temps) -> Self {
        mstp::start(Mstpcr::D, 22);
        unsafe { write_volatile(TSCR, 0) };
        TemperatureSensor { _temps: temps }
    }

    /// The factory calibration: the 12-bit results at Tj = 125 °C (TSCDR)
    /// and -40 °C (TSCDR2), with AVCC0 = 3.3 V.
    pub fn calibration(&self) -> (u16, u16) {
        unsafe { ((read_volatile(TSCDR) & 0x0fff) as u16, (read_volatile(TSCDR2) & 0x0fff) as u16) }
    }

    /// Converts the sensor output (manual figure 62.3): TSEN, 30 us, TSOE,
    /// a conversion with the 5 us sampling of table 1, then the sensor off
    /// again.
    pub fn read_raw(&mut self, adc: &mut Adc) -> Result<u16, ConversionError> {
        unsafe { write_volatile(TSCR, TSEN) };
        spin_us(adc.cpuclk, 30);
        unsafe { write_volatile(TSCR, TSEN | TSOE) };
        let r = adc.convert(CNVCS_TEMPERATURE, false, 1, FORMAT_12_UNSIGNED, ADEXDR0 + 4 * 4);
        unsafe {
            write_volatile(TSCR, TSEN);
            write_volatile(TSCR, 0);
        }
        data(r)
    }

    /// The die temperature in millidegrees Celsius from one conversion, on
    /// the line through the two calibration points: T = -40 + 165 x (raw -
    /// TSCDR2) / (TSCDR - TSCDR2). It assumes VREFH0 = AVCC0 = 3.3 V, the
    /// calibration's condition.
    pub fn read_millicelsius(&mut self, adc: &mut Adc) -> Result<i32, ConversionError> {
        let raw = self.read_raw(adc)? as i32;
        let (c125, c40) = self.calibration();
        let (c125, c40) = (c125 as i32, c40 as i32);
        Ok(-40_000 + 165_000 * (raw - c40) / (c125 - c40))
    }
}
