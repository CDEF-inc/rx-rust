//! rx74-hal on the EK-RX74N board: PWM on GPTW8 out of PF05 (GTIOC8B, the
//! user LED LED1), with PLL1 at 480 MHz (PCLKD 120 MHz), measured on the pin
//! itself without wiring. A loop reads PF05 through PIDR and CMT0 (PCLKB /
//! 8 = 7.5 MHz): the CMT0 time
//! between two reads counts as high when the first read was high, so the
//! duty cycle does not depend on how long the loop takes for either level
//! (a count of samples did: 2-3 % low in the dev build). The rising edges
//! per time give the frequency. At 1 kHz: duty 0, 25, 50, 75 % and full; at
//! 100 kHz: 50 %. LED1 is left glowing dimly (5 % at 1 kHz). Booted from
//! the code MRAM by scripts/hw-run.sh; the results are in `checks` /
//! `failures` / `first_failure` / `finished`, the measurements in
//! `pwm_values`.

#![no_std]
#![no_main]
// The result variables keep the names the test scripts read.
#![allow(non_upper_case_globals)]

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

use embedded_hal::pwm::SetDutyCycle;
use rx74_hal::clock::Config;
use rx74_hal::gpio::Parts;
use rx74_hal::pac::Peripherals;
use rx74_hal::pwm::{Pwm, Timing};

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut first_failure: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;
// Per measurement: high time per mille, rising edges. 1 kHz duty 25, 50,
// 75 %; 100 kHz 50 %; then the reads of PF05 in the 1 kHz 50 % run.
#[unsafe(no_mangle)]
static mut pwm_values: [u32; 9] = [0; 9];

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
    unsafe { write_volatile(&raw mut pwm_values[i], v) };
}

fn rd32(a: usize) -> u32 {
    unsafe { read_volatile(a as *const u32) }
}

/// The level of PF05 (PIDR of port F, 16 bits).
fn pf05() -> bool {
    let pidr = unsafe { read_volatile(0x87c0_01e4_usize as *const u16) };
    pidr & 1 << 5 != 0
}

/// A measurement of PF05 over a time, in CMT0 counts.
struct Samples {
    high: u64,
    total: u64,
    reads: u32,
    rising: u32,
}

impl Samples {
    /// High time per mille.
    fn permille(&self) -> u32 {
        (self.high * 1000 / self.total) as u32
    }
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = Config::pll_480mhz_from_24mhz().configure(&p.sysc, &p.sram, &p.mram);
    let pins = Parts::take().unwrap();

    // --- the timing choice: the smallest prescaler with a period of at
    // most 65535 counts of GTCLK = PCLKD (120 MHz)
    let t = |hz| Timing::new(120_000_000, hz);
    check(1, t(1_000) == Some(Timing { tpcs: 1, period: 60_000, hz: 1_000 }));
    check(2, t(50) == Some(Timing { tpcs: 6, period: 37_500, hz: 50 }));
    check(3, t(1_000_000) == Some(Timing { tpcs: 0, period: 120, hz: 1_000_000 }));
    check(4, t(2) == Some(Timing { tpcs: 10, period: 58_594, hz: 1 }));
    check(5, t(1).is_none() && t(0).is_none() && t(100_000_000).is_none());

    // --- CMT0 at PCLKB / 8 = 7.5 MHz, free running, for the sampling
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.mstpreg.mstpcrf().modify(|_, w| w.mstpf15().clear_bit());
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) });
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    p.cmt0.cmcr().write(|w| unsafe { w.cks().bits(0) });
    p.cmt0.cmcor().write(|w| unsafe { w.bits(0xffff) });
    p.cmt.cmstr0().write(|w| w.str0().set_bit());
    // Samples PF05 for `us` microseconds. The counter wraps every 8.7 ms;
    // the loop reads it far more often.
    let sample = |us: u32| {
        let counts = us as u64 * 7_500_000 / 1_000_000;
        let mut s = Samples { high: 0, total: 0, reads: 0, rising: 0 };
        let mut last = p.cmt0.cmcnt().read().bits();
        let mut prev = pf05();
        while s.total < counts {
            let now = p.cmt0.cmcnt().read().bits();
            let d = now.wrapping_sub(last) as u64;
            last = now;
            s.total += d;
            if prev {
                s.high += d;
            }
            let h = pf05();
            s.reads += 1;
            if h && !prev {
                s.rising += 1;
            }
            prev = h;
        }
        s
    };

    // --- GPTW8 at 1 kHz on PF05
    let mut pwm = Pwm::gptw8(p.gptw8, pins.pf05, 1_000, &clocks);
    check(6, pwm.timing() == Timing { tpcs: 1, period: 60_000, hz: 1_000 });
    check(7, pwm.max_duty_cycle() == 60_000);
    // PF05PFS: PSEL = 3 (GTIOC8B), PMR
    let pfs = rd32(0x87c0_0800 + 0x40 * 0x0f + 4 * 5);
    check(8, pfs >> 24 & 0x7f == 3 && pfs & 1 << 16 != 0);
    // GTCLKCR.BPEN; GTCR: GTCLK / 2, sawtooth-wave PWM mode 1, CST; GTIOR:
    // OBE, GTIOB = 01001b; GTBER.CCRB = 01b; GTPR = 59999
    let g = |off: usize| rd32(0x8742_c800 + off);
    check(9, rd32(0x8742_bf10) == 1 && g(0x2c) == 0x0080_0001);
    check(10, g(0x34) == 0x0109_0000 && g(0x40) == 0x0004_0000 && g(0x64) == 59_999);

    // Duty 0 and full: constant levels (after a period for the switch).
    let wait = |ms: u32| {
        let _ = sample(ms * 1000);
    };
    pwm.set_duty_cycle(0).unwrap();
    wait(2);
    let s = sample(20_000);
    check(11, s.high == 0 && s.reads > 10_000);
    pwm.set_duty_cycle_fully_on().unwrap();
    wait(2);
    let s = sample(20_000);
    check(12, s.high == s.total);

    // 25, 50, 75 %: the share of high samples within 1.5 %, 100 rising
    // edges in 100 ms
    for (i, pct) in [25u8, 50, 75].into_iter().enumerate() {
        pwm.set_duty_cycle_percent(pct).unwrap();
        wait(2);
        let s = sample(100_000);
        record(2 * i, s.permille());
        record(2 * i + 1, s.rising);
        let want = pct as u32 * 10;
        check(13 + i as u32, s.permille().abs_diff(want) <= 15);
        check(16 + i as u32, (99..=101).contains(&s.rising));
        if pct == 50 {
            record(8, s.reads);
        }
    }

    // --- 100 kHz (period 1200 counts at GTCLK / 1), 50 %: 1000 rising
    // edges in 10 ms
    let (gptw8, pf05_pin) = pwm.free();
    let mut pwm = Pwm::gptw8(gptw8, pf05_pin, 100_000, &clocks);
    check(19, pwm.timing() == Timing { tpcs: 0, period: 1_200, hz: 100_000 });
    pwm.set_duty_cycle_percent(50).unwrap();
    wait(1);
    let s = sample(10_000);
    record(6, s.permille());
    record(7, s.rising);
    check(20, s.permille().abs_diff(500) <= 30);
    check(21, (990..=1010).contains(&s.rising));

    // Leave LED1 glowing dimly: 1 kHz, 5 %.
    let (gptw8, pf05_pin) = pwm.free();
    let mut pwm = Pwm::gptw8(gptw8, pf05_pin, 1_000, &clocks);
    pwm.set_duty_cycle_percent(5).unwrap();
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
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
