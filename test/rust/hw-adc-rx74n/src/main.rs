//! rx74n-hal on the EK-RX74N board: the 16-bit A/D converter (ADC16H,
//! unit 0, SAR) without wiring, with PLL1 at 480 MHz (ADCLK = PLL1P / 8 =
//! 60 MHz). The ADCCLK and module set-up, the calibration, the
//! self-diagnosis voltages, the internal reference voltage, the
//! temperature sensor (between its two calibration points), D/A converter
//! 0 through its internal output at three levels, and an analog pin (P400
//! = AN000; its value is not checked: nothing drives it). Booted from the
//! code MRAM by scripts/hw-run.sh; the results are in `checks` / `failures`
//! / `first_failure` / `finished`, the measured values in `adc_values`.

#![no_std]
#![no_main]
// The result variables keep the names the test scripts read.
#![allow(non_upper_case_globals)]

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

use rx74n_hal::adc::{Adc, DiagVoltage, TemperatureSensor};
use rx74n_hal::clock::Config;
use rx74n_hal::gpio::Parts;
use rx74n_hal::pac::Peripherals;

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut first_failure: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;
// Self-diagnosis zero / negative / positive; the internal reference;
// TSCDR (125 °C) / TSCDR2 (-40 °C); the temperature sensor; the
// temperature in m°C; DA0 at 400h / 800h / C00h; AN000.
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

fn rd32(a: usize) -> u32 {
    unsafe { read_volatile(a as *const u32) }
}

fn wr32(a: usize, v: u32) {
    unsafe { write_volatile(a as *mut u32, v) }
}

/// Waits at least `us` microseconds at CPUCLK0 480 MHz (at least one
/// cycle per loop; ICLK is 240 MHz, but the CPU runs at CPUCLK0).
fn wait_us(us: u32) {
    for _ in 0..us * 480 {
        core::hint::spin_loop();
    }
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = Config::pll_480mhz_from_24mhz().configure(&p.sysc, &p.sram, &p.mram);
    let pins = Parts::take().unwrap();

    // --- set-up: ADCCLK = PLL1P / 8 (ADCCKCR = 05h, ADCCKDIVCR = 04h),
    // MSTPD21 released, units 1 and 2 asleep, ADCLK running; conversion
    // time 6 states (100 ns), sampling tables 0 / 1 = 60 / 300 states
    let mut adc = Adc::adc16h(p.adc_b, &clocks);
    check(1, clocks.pll1p == 480_000_000 && adc.adclk() == 60_000_000);
    check(2, rd8(0x8701_905b) == 0x05 && rd8(0x8701_905a) == 0x04);
    check(3, rd32(0x8780_300c) & 1 << 21 == 0);
    check(4, rd32(0x8743_8014) == 1 && rd32(0x8743_8018) == 1 && rd32(0x8743_8004) & 1 == 1);
    check(5, rd32(0x8743_8260) == 0x0006_0606 && rd32(0x8743_8240) == 300 << 16 | 60);
    // the calibration ended without an error (ADERSR.ADERF0)
    check(6, rd32(0x8743_8c88) & 1 == 0);

    // --- self-diagnosis: differential, 16-bit signed; ideally 0, -32768,
    // 32767
    let zero = adc.self_diagnosis(DiagVoltage::Zero);
    let neg = adc.self_diagnosis(DiagVoltage::Negative);
    let pos = adc.self_diagnosis(DiagVoltage::Positive);
    for (i, r) in [zero, neg, pos].iter().enumerate() {
        record(i, r.map_or(0xdead_0000, |v| v as u16 as u32));
    }
    check(7, zero.is_ok_and(|v| v.unsigned_abs() <= 0x100));
    check(8, neg.is_ok_and(|v| v <= -32768 + 0x100));
    check(9, pos.is_ok_and(|v| v >= 32767 - 0x100));
    // DIAGVAL back to 0
    check(10, rd32(0x8743_8200) == 0);

    // --- the internal reference, 0.77-0.84 V of VREFH0 (3.3 V on the board;
    // allow 3.2-3.4 V), 12 bits: 927-1075
    let vref = adc.read_internal_reference();
    record(3, vref.map_or(0xdead_0000, |v| v as u32));
    check(11, vref.is_ok_and(|v| (927..=1075).contains(&v)));

    // --- the temperature sensor: TSCDR (125 °C) and TSCDR2 (-40 °C) differ,
    // the board is at room temperature
    let mut temp = TemperatureSensor::new(p.temps);
    let (c125, c40) = temp.calibration();
    record(4, c125 as u32);
    record(5, c40 as u32);
    check(12, c125 != c40 && c125 > 0 && c40 > 0);
    let raw = temp.read_raw(&mut adc);
    record(6, raw.map_or(0xdead_0000, |v| v as u32));
    let mc = temp.read_millicelsius(&mut adc);
    record(7, mc.map_or(0xdead_0000, |v| v as u32));
    check(13, mc.is_ok_and(|v| (5_000..=70_000).contains(&v)));
    // the sensor off again (TSCR = 0)
    check(14, rd8(0x8783_5000) == 0);

    // --- D/A converter 0 through its internal output (DACR0.DAOUTDIS = 1,
    // then DACEN), at 400h, 800h, C00h: the A/D result within 100 of it
    // (both refer to 3.3 V on the board)
    // MSTPCRD.MSTPD20 (not write-protected; no interrupts run here)
    wr32(0x8780_300c, rd32(0x8780_300c) & !(1 << 20));
    wr32(0x8783_3004, 1 << 31);
    let mut da_ok = true;
    for (i, v) in [0x400u16, 0x800, 0xc00].into_iter().enumerate() {
        unsafe { write_volatile(0x8783_3000_usize as *mut u16, v) };
        wr32(0x8783_3004, 1 << 31 | 1);
        wait_us(10);
        let r = adc.read_da(0);
        record(8 + i, r.map_or(0xdead_0000, |x| x as u32));
        da_ok &= r.is_ok_and(|x| x.abs_diff(v) <= 100);
    }
    check(15, da_ok);
    wr32(0x8783_3004, 1 << 31);

    // --- an analog pin: P400 = AN000; PFS: ASEL, PMR / PDR / PCR clear
    let p400 = pins.p400.into_analog();
    let pfs = rd32(0x87c0_0900);
    check(16, pfs & 1 << 15 != 0 && pfs & (1 << 16 | 1 << 4 | 1 << 2) == 0);
    let an000 = adc.read(&p400);
    record(11, an000.map_or(0xdead_0000, |v| v as u32));
    check(17, an000.is_ok_and(|v| v <= 0xfff));
    // a longer sampling time for table 0
    adc.set_sampling_states(120);
    check(18, rd32(0x8743_8240) == 300 << 16 | 120 && adc.read(&p400).is_ok());
    // back to a digital input: ASEL cleared
    let _p400 = p400.into_input();
    check(19, rd32(0x87c0_0900) & 1 << 15 == 0);

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
