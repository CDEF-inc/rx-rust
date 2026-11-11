//! rx72-hal on the AP-RX72M-0A board: the 12-bit A/D converter without
//! wiring. Both units convert their self-diagnosis voltages (0 V, half
//! the reference, the reference); unit 1 converts the internal reference
//! voltage and the temperature sensor (PCLKD 30 MHz, so that the 5 us
//! sampling time fits); and analog pins of both units are set up and
//! converted (their values are not checked: nothing drives them). Booted
//! from the code flash by scripts/hw-run.sh; the results are in `checks` /
//! `failures` / `first_failure` / `finished`, the measured values in
//! `adc_values`.

#![no_std]
#![no_main]
// The result variables keep the names the test scripts read.
#![allow(non_upper_case_globals)]

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

use rx72_hal::adc::{Adc, DiagVoltage, TemperatureSensor};
use rx72_hal::clock::{Config, Div};
use rx72_hal::gpio::Parts;
use rx72_hal::pac::Peripherals;

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut first_failure: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;
// Unit 0 / unit 1 self-diagnosis 0 V, 1/2, full; the internal reference;
// TSCDR; the temperature sensor; the temperature in m°C; AN000; AN100.
#[unsafe(no_mangle)]
static mut adc_values: [u32; 12] = [0; 12];

fn check(id: u32, ok: bool) {
    unsafe {
        checks += 1;
        if !ok {
            if failures == 0 {
                first_failure = id;
            }
            failures += 1;
        }
    }
}

fn record(i: usize, v: u32) {
    unsafe { write_volatile(&raw mut adc_values[i], v) };
}

fn rd8(a: usize) -> u8 {
    unsafe { read_volatile(a as *const u8) }
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // PCLKD 30 MHz: unit 1's ADCLK, slow enough for the 5 us sampling of
    // the temperature sensor and the internal reference (150 states).
    let config = Config { pclkd: Div::Div8, ..Config::pll_240mhz_from_24mhz() };
    let clocks = config.configure(&p.system);
    check(1, clocks.pclkc == 60_000_000 && clocks.pclkd == 30_000_000);
    let pins = Parts::take().unwrap();

    let mut adc0 = Adc::s12ad(p.s12ad, &clocks);
    let mut adc1 = Adc::s12ad1(p.s12ad1, &clocks);
    check(2, adc0.adclk() == 60_000_000 && adc1.adclk() == 30_000_000);
    // Module stop released: MSTPCRA.MSTPA17 / MSTPA16
    check(3, unsafe { read_volatile(0x0008_0010 as *const u32) } & 0b11 << 16 == 0);
    // Sampling time of the electrical characteristics: unit 0 16 states at
    // 60 MHz (ADSSTR0-7), unit 1 38 at 60 MHz scaled to 30 MHz = 19
    // (ADSSTR0-15, ADSSTRL)
    check(25, adc0.rated_sampling_states() == 16 && adc1.rated_sampling_states() == 19);
    check(26, (0x0008_90e0..=0x0008_90e7).all(|a| rd8(a) == 16));
    check(27, (0x0008_91e0..=0x0008_91ef).all(|a| rd8(a) == 19) && rd8(0x0008_91dd) == 19);

    // --- self-diagnosis: the status says which voltage was converted
    for (unit, base) in [(0usize, 4u32), (1, 8)] {
        let mut diag = |v| if unit == 0 { adc0.self_diagnosis(v) } else { adc1.self_diagnosis(v) };
        let zero = diag(DiagVoltage::Zero);
        let half = diag(DiagVoltage::Half);
        let full = diag(DiagVoltage::Full);
        record(3 * unit, zero.value as u32);
        record(3 * unit + 1, half.value as u32);
        record(3 * unit + 2, full.value as u32);
        check(base, zero.voltage == Some(DiagVoltage::Zero) && zero.value <= 0x20);
        check(base + 1, half.voltage == Some(DiagVoltage::Half));
        check(base + 2, (0x7c0..=0x840).contains(&half.value));
        check(base + 3, full.voltage == Some(DiagVoltage::Full) && full.value >= 0xfc0);
    }

    // --- the internal reference voltage, 1.13-1.23 V of AVCC1 (3.3 V on
    // the board; allow 3.2-3.4 V): 1361-1574
    let vref = adc1.read_internal_reference();
    record(6, vref as u32);
    check(12, (1361..=1574).contains(&vref));
    // ADSSTRO = 5 us at 30 MHz
    check(13, rd8(0x0008_91df) == 150);

    // --- the temperature sensor: TSCDR is the result at 125 °C (about
    // 1.61 V: 1.21 V at 25 °C + 4 mV/°C), the board is at room temperature
    let mut temp = TemperatureSensor::new(p.temps, p.tempsconst);
    let cal = temp.calibration();
    record(7, cal as u32);
    check(14, (1700..=2300).contains(&cal));
    let raw = temp.read_raw(&mut adc1);
    record(8, raw as u32);
    // At 25 °C the sensor gives about 1.21 V: 1500 counts.
    check(15, raw < cal && (1300..=1800).contains(&raw));
    let mc = temp.read_millicelsius(&mut adc1);
    record(9, mc as u32);
    check(16, (5_000..=70_000).contains(&mc));
    // ADSSTRT = 5 us, and the sensor is off again (TSCR = 0)
    check(17, rd8(0x0008_91de) == 150 && rd8(0x0008_c500) == 0);

    // --- analog pins: P40 = AN000 (unit 0), PE2 = AN100 (unit 1)
    let p40 = pins.p40.into_analog();
    let pe2 = pins.pe2.into_analog();
    // PFS = 80h (ASEL), PMR and PDR clear
    check(18, rd8(0x0008_c160) == 0x80 && rd8(0x0008_c1b2) == 0x80);
    check(19, rd8(0x0008_c064) & 1 == 0 && rd8(0x0008_c004) & 1 == 0);
    check(20, rd8(0x0008_c06e) & 1 << 2 == 0 && rd8(0x0008_c00e) & 1 << 2 == 0);
    let an000 = adc0.read(&p40);
    let an100 = adc1.read(&pe2);
    record(10, an000 as u32);
    record(11, an100 as u32);
    check(21, an000 <= 0xfff && an100 <= 0xfff);
    // The channel selection is cleared after the read, and ADST is clear.
    let rd16 = |a| unsafe { read_volatile(a as *const u16) };
    check(22, rd16(0x0008_9004) == 0 && rd16(0x0008_9104) == 0);
    check(23, rd16(0x0008_9000) & 1 << 15 == 0 && rd16(0x0008_9100) & 1 << 15 == 0);
    // A longer sampling time for every channel of unit 0
    adc0.set_sampling_states(40);
    let _ = adc0.read(&p40);
    check(28, (0x0008_90e0..=0x0008_90e7).all(|a| rd8(a) == 40));
    // Back to a digital input: ASEL cleared
    let _p40 = p40.into_input();
    check(24, rd8(0x0008_c160) == 0);

    unsafe { write_volatile(&raw mut finished, 1) };
    done()
}

// The end of the test (the board runner's breakpoint).
#[unsafe(no_mangle)]
#[inline(never)]
extern "C" fn done() -> ! {
    loop {
        rx_rt::interrupt::wait();
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe {
        failures += 1;
        first_failure = 999;
    }
    loop {}
}
