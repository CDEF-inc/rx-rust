//! The 12-bit A/D converter (S12ADFa): unit 0 (S12AD, AN000-AN007 on
//! P40-P47, ADCLK = PCLKC) and unit 1 (S12AD1, AN100-AN120, the internal
//! reference voltage and the temperature sensor, ADCLK = PCLKD), one
//! conversion at a time.
//!
//! embedded-hal 1.0 has no A/D converter trait, so [`Adc::read`] takes an
//! analog pin (`Pin::into_analog`) of the unit and returns the 12-bit
//! result, right-aligned. Each read is a single scan of that channel
//! started by software (ADCSR.ADST), polled until ADST clears. The sampling
//! time of the channels stays at its reset value (11 ADCLK states).
//!
//! [`Adc::self_diagnosis`] converts one of the unit's internal voltages
//! (0 V, half the reference, the reference). Unit 1 also converts its
//! internal reference voltage ([`Adc::read_internal_reference`], 1.18 V
//! typical) and, with [`TemperatureSensor`], the temperature sensor. Both
//! need a sampling time of at least 5 us (manual 58.2.18), which the 8-bit
//! sampling state registers only give with ADCLK up to 51 MHz: set PCLKD
//! to 30 MHz (`clock::Div::Div8` at 240 MHz) to use them.
//!
//! The two units have the same register layout at 0x89000 and 0x89100,
//! which the driver addresses directly.

use core::marker::PhantomData;
use core::ptr::{read_volatile, write_volatile};

use crate::clock::Clocks;
use crate::gpio::{Analog, Pin};
use crate::mstp::{self, Mstpcr};
use crate::pac;

// Register offsets from the unit's base.
const ADCSR: usize = 0x00;
const ADANSA0: usize = 0x04;
const ADANSA1: usize = 0x06;
const ADCER: usize = 0x0e;
const ADEXICR: usize = 0x12;
const ADTSDR: usize = 0x1a;
const ADOCDR: usize = 0x1c;
const ADRD: usize = 0x1e;
const ADDR0: usize = 0x20;
const ADSAMPR: usize = 0x63;
const ADSAM: usize = 0x6e;
const ADSSTRT: usize = 0xde;
const ADSSTRO: usize = 0xdf;

const ADST: u16 = 1 << 15;
const DIAGM: u16 = 1 << 11;
const DIAGLD: u16 = 1 << 10;
const TSSA: u16 = 1 << 8;
const OCSA: u16 = 1 << 9;

const TSCR: *mut u8 = 0x0008_c500 as _;
const TSCDR: *const u32 = 0xfe7f_7d7c_usize as _;
const TSEN: u8 = 1 << 7;
const TSOE: u8 = 1 << 4;

mod sealed {
    pub trait Sealed {}
}

/// An A/D converter unit.
pub trait Unit: sealed::Sealed {
    #[doc(hidden)]
    const BASE: usize;
    /// The unit's PAC peripheral.
    type Periph;
}

/// Unit 0 (S12AD): AN000-AN007, high speed.
pub struct Unit0;
/// Unit 1 (S12AD1): AN100-AN120, the internal reference voltage and the
/// temperature sensor, medium speed.
pub struct Unit1;

impl sealed::Sealed for Unit0 {}
impl sealed::Sealed for Unit1 {}
impl Unit for Unit0 {
    const BASE: usize = 0x0008_9000;
    type Periph = pac::S12ad;
}
impl Unit for Unit1 {
    const BASE: usize = 0x0008_9100;
    type Periph = pac::S12ad1;
}

/// An analog pin of unit `U` and its channel number (`n` of ANxnn).
pub trait Channel<U> {
    const CHANNEL: u8;
}

macro_rules! channels {
    ($unit:ty: $($port:literal, $bit:literal => $ch:literal;)*) => {
        $(impl Channel<$unit> for Pin<$port, $bit, Analog> {
            const CHANNEL: u8 = $ch;
        })*
    };
}

channels! { Unit0:
    0x04, 0 => 0; 0x04, 1 => 1; 0x04, 2 => 2; 0x04, 3 => 3;
    0x04, 4 => 4; 0x04, 5 => 5; 0x04, 6 => 6; 0x04, 7 => 7;
}

channels! { Unit1:
    0x0e, 2 => 0; 0x0e, 3 => 1; 0x0e, 4 => 2; 0x0e, 5 => 3;
    0x0e, 6 => 4; 0x0e, 7 => 5; 0x0d, 6 => 6; 0x0d, 7 => 7;
    0x0d, 0 => 8; 0x0d, 1 => 9; 0x0d, 2 => 10; 0x0d, 3 => 11;
    0x0d, 4 => 12; 0x0d, 5 => 13; 0x09, 0 => 14; 0x09, 1 => 15;
    0x09, 2 => 16; 0x09, 3 => 17; 0x00, 0 => 18; 0x00, 1 => 19;
    0x00, 2 => 20;
}

/// The voltage of a self-diagnosis (ADCER.DIAGVAL).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DiagVoltage {
    /// 0 V
    Zero = 0b01,
    /// The reference voltage x 1/2
    Half = 0b10,
    /// The reference voltage
    Full = 0b11,
}

/// The result of a self-diagnosis (ADRD).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diagnosis {
    /// The voltage the unit says it converted (ADRD bits 15-14; `None`
    /// for 00b, no self-diagnosis since power-on).
    pub voltage: Option<DiagVoltage>,
    /// The 12-bit conversion result.
    pub value: u16,
}

/// An A/D converter unit, converting one input at a time.
pub struct Adc<U: Unit> {
    _adc: U::Periph,
    /// The unit's ADCLK and ICLK in Hz.
    adclk: u32,
    iclk: u32,
    _unit: PhantomData<U>,
}

fn rd16(a: usize) -> u16 {
    unsafe { read_volatile(a as *const u16) }
}

fn wr16(a: usize, v: u16) {
    unsafe { write_volatile(a as *mut u16, v) }
}

/// Waits at least `us` microseconds (a loop of at least one ICLK cycle
/// per iteration).
fn spin_us(iclk: u32, us: u32) {
    let mut n = iclk / 1_000_000 * us;
    while n > 0 {
        core::hint::spin_loop();
        n -= 1;
    }
}

impl Adc<Unit0> {
    /// Sets up unit 0 (taking it out of module stop, MSTPCRA.MSTPA17).
    ///
    /// Panics if PCLKB : PCLKC is not 1, 2, 4 or 8.
    pub fn s12ad(adc: pac::S12ad, clocks: &Clocks) -> Self {
        Self::init(adc, 17, clocks.pclkc, clocks)
    }
}

impl Adc<Unit1> {
    /// Sets up unit 1 (taking it out of module stop, MSTPCRA.MSTPA16) with
    /// the medium speed conversion time (ADSAM.SAM = 1, as the manual asks
    /// for this unit).
    ///
    /// Panics if PCLKB : PCLKD is not 1, 2, 4 or 8.
    pub fn s12ad1(adc: pac::S12ad1, clocks: &Clocks) -> Self {
        let adc = Self::init(adc, 16, clocks.pclkd, clocks);
        // ADSAM is protected by ADSAMPR and written with interrupts
        // disabled (critical-section clears PSW.I).
        critical_section::with(|_| unsafe {
            write_volatile((Unit1::BASE + ADSAMPR) as *mut u8, 0b11);
            wr16(Unit1::BASE + ADSAM, 1 << 5);
            write_volatile((Unit1::BASE + ADSAMPR) as *mut u8, 0b10);
        });
        adc
    }

    /// Converts the internal reference voltage (ADEXICR.OCSA, ADOCDR),
    /// 1.13-1.23 V. Its sampling time (ADSSTRO) is set to 5 us.
    ///
    /// Panics if ADCLK (PCLKD) is above 51 MHz.
    pub fn read_internal_reference(&mut self) -> u16 {
        unsafe { write_volatile((Unit1::BASE + ADSSTRO) as *mut u8, self.states_5us()) };
        wr16(Unit1::BASE + ADEXICR, OCSA);
        // 400 ns after OCSA before the conversion starts.
        spin_us(self.iclk, 1);
        self.scan();
        let v = rd16(Unit1::BASE + ADOCDR);
        wr16(Unit1::BASE + ADEXICR, 0);
        v
    }

    /// ADCLK states for a 5 us sampling time, for the temperature sensor
    /// and the internal reference voltage.
    fn states_5us(&self) -> u8 {
        let states = (self.adclk as u64 * 5).div_ceil(1_000_000);
        assert!(states <= 255, "ADCLK too fast for a 5 us sampling time");
        states as u8
    }
}

impl<U: Unit> Adc<U> {
    fn init(adc: U::Periph, mstpa: u32, adclk: u32, clocks: &Clocks) -> Self {
        assert!(matches!(clocks.pclkb / adclk, 1 | 2 | 4 | 8) && clocks.pclkb % adclk == 0);
        mstp::start(Mstpcr::A, mstpa);
        // 1 us after leaving module stop before converting.
        spin_us(clocks.iclk, 1);
        // Single scan, software trigger, no interrupt; 12 bits,
        // right-aligned, no self-diagnosis; no channel.
        wr16(U::BASE + ADCSR, 0);
        wr16(U::BASE + ADCER, 0);
        wr16(U::BASE + ADANSA0, 0);
        if U::BASE == Unit1::BASE {
            wr16(U::BASE + ADANSA1, 0);
            wr16(U::BASE + ADEXICR, 0);
        }
        Adc { _adc: adc, adclk, iclk: clocks.iclk, _unit: PhantomData }
    }

    /// The unit's A/D conversion clock in Hz.
    pub fn adclk(&self) -> u32 {
        self.adclk
    }

    /// Starts a single scan and waits for its end (ADST cleared).
    fn scan(&mut self) {
        wr16(U::BASE + ADCSR, ADST);
        while rd16(U::BASE + ADCSR) & ADST != 0 {}
    }

    /// Converts the input of `pin` and returns the 12-bit result.
    pub fn read<PIN: Channel<U>>(&mut self, _pin: &PIN) -> u16 {
        let ch = PIN::CHANNEL as usize;
        let (ansa, bit) = if ch < 16 { (ADANSA0, ch) } else { (ADANSA1, ch - 16) };
        wr16(U::BASE + ansa, 1 << bit);
        self.scan();
        wr16(U::BASE + ansa, 0);
        rd16(U::BASE + ADDR0 + 2 * ch)
    }

    /// Converts the internal voltage `v` (ADCER.DIAGM with DIAGLD, the
    /// fixed voltage mode; no channel selected) and returns ADRD.
    pub fn self_diagnosis(&mut self, v: DiagVoltage) -> Diagnosis {
        wr16(U::BASE + ADCER, DIAGM | DIAGLD | (v as u16) << 8);
        self.scan();
        wr16(U::BASE + ADCER, 0);
        let r = rd16(U::BASE + ADRD);
        let voltage = match r >> 14 {
            0b01 => Some(DiagVoltage::Zero),
            0b10 => Some(DiagVoltage::Half),
            0b11 => Some(DiagVoltage::Full),
            _ => None,
        };
        Diagnosis { voltage, value: r & 0x0fff }
    }
}

/// The temperature sensor (TEMPS), converted by unit 1.
pub struct TemperatureSensor {
    _temps: pac::Temps,
    _calibration: pac::Tempsconst,
}

impl TemperatureSensor {
    /// Takes the sensor out of module stop (MSTPCRB.MSTPB8); it stays off
    /// between reads.
    pub fn new(temps: pac::Temps, calibration: pac::Tempsconst) -> Self {
        mstp::start(Mstpcr::B, 8);
        unsafe { write_volatile(TSCR, 0) };
        TemperatureSensor { _temps: temps, _calibration: calibration }
    }

    /// The factory calibration (TSCDR): the conversion result at Tj =
    /// 125 °C with AVCC1 = 3.3 V.
    pub fn calibration(&self) -> u16 {
        (unsafe { read_volatile(TSCDR) } & 0x0fff) as u16
    }

    /// Converts the sensor output (manual figure 60.3): TSEN, 30 us, TSOE,
    /// a single scan of ADEXICR.TSSA with a 5 us sampling time (ADSSTRT),
    /// then the sensor off again. Returns ADTSDR.
    ///
    /// Panics if unit 1's ADCLK (PCLKD) is above 51 MHz.
    pub fn read_raw(&mut self, adc: &mut Adc<Unit1>) -> u16 {
        let states = adc.states_5us();
        unsafe {
            write_volatile((Unit1::BASE + ADSSTRT) as *mut u8, states);
            write_volatile(TSCR, TSEN);
        }
        spin_us(adc.iclk, 30);
        unsafe { write_volatile(TSCR, TSEN | TSOE) };
        wr16(Unit1::BASE + ADEXICR, TSSA);
        adc.scan();
        let v = rd16(Unit1::BASE + ADTSDR);
        wr16(Unit1::BASE + ADEXICR, 0);
        unsafe {
            write_volatile(TSCR, TSEN);
            write_volatile(TSCR, 0);
        }
        v
    }

    /// The die temperature in millidegrees Celsius from one conversion,
    /// with the calibration point and the typical slope (4 mV/°C):
    /// T = 125 + (raw - TSCDR) x 3.3 V / 4096 / 4 mV. It assumes AVCC1 =
    /// 3.3 V, the calibration's condition; a single-point calibration is
    /// within about ±7 °C (manual figure 60.2).
    pub fn read_millicelsius(&mut self, adc: &mut Adc<Unit1>) -> i32 {
        let raw = self.read_raw(adc) as i32;
        125_000 + (raw - self.calibration() as i32) * 3_300_000 / (4096 * 4)
    }
}
