//! rx72-hal on the AP-RX72M-0A board: PWM on MTU6 out of PJ1 (MTIOC6A,
//! the monitor LED LD2), measured on the pin itself without wiring. A loop
//! reads PJ1 through PIDR and CMT0 (PCLKB / 8 = 7.5 MHz): the CMT0 time
//! between two reads counts as high when the first read was high, so the
//! duty cycle does not depend on how long the loop takes for either level
//! (a count of samples did: 2-3 % low in the dev build). The rising edges
//! per time give the frequency. At 1 kHz: duty 0, 25, 50, 75 % and full; at
//! 100 kHz: 50 %. LD2 is left glowing dimly (5 % at 1 kHz). Booted from
//! the code flash by scripts/hw-run.sh; the results are in `checks` /
//! `failures` / `first_failure` / `finished`, the measurements in
//! `pwm_values`.

use core::ptr::{read_volatile, write_volatile};

use embedded_hal::pwm::SetDutyCycle;
use hw_board::hal::clock::Config;
use hw_board::hal::gpio::Parts;
use hw_board::hal::pac::Peripherals;
use hw_board::hal::pwm::{Pwm, Timing};
use hw_board::check;

// Per measurement: high time per mille, rising edges. 1 kHz duty 25, 50,
// 75 %; 100 kHz 50 %; then the reads of PJ1 in the 1 kHz 50 % run.
#[unsafe(no_mangle)]
static mut pwm_values: [u32; 9] = [0; 9];

fn record(i: usize, v: u32) {
    unsafe { write_volatile(&raw mut pwm_values[i], v) };
}

fn rd8(a: usize) -> u8 {
    unsafe { read_volatile(a as *const u8) }
}

/// The level of PJ1 (PIDR of port J).
fn pj1() -> bool {
    rd8(0x0008_c052) & 1 << 1 != 0
}

/// A measurement of PJ1 over a time, in CMT0 counts.
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
    let clocks = Config::pll_240mhz_from_24mhz().configure(&p.system);
    let pins = Parts::take().unwrap();

    // --- the timing choice: the smallest prescaler with a period of at
    // most 65535 counts of PCLKA (120 MHz)
    let t = |hz| Timing::new(120_000_000, hz);
    check(1, t(1_000) == Some(Timing { div: 2, period: 60_000, hz: 1_000 }));
    check(2, t(50) == Some(Timing { div: 64, period: 37_500, hz: 50 }));
    check(3, t(1_000_000) == Some(Timing { div: 1, period: 120, hz: 1_000_000 }));
    check(4, t(2) == Some(Timing { div: 1024, period: 58_594, hz: 1 }));
    check(5, t(1).is_none() && t(0).is_none() && t(100_000_000).is_none());

    // --- CMT0 at PCLKB / 8 = 7.5 MHz, free running, for the sampling
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.system.mstpcra().modify(|_, w| w.mstpa15().clear_bit());
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) });
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    p.cmt0.cmcr().write(|w| unsafe { w.cks().bits(0) });
    p.cmt0.cmcor().write(|w| unsafe { w.bits(0xffff) });
    p.cmt.cmstr0().write(|w| w.str0().set_bit());
    // Samples PJ1 for `us` microseconds. The counter wraps every 8.7 ms;
    // the loop reads it far more often.
    let sample = |us: u32| {
        let counts = us as u64 * 7_500_000 / 1_000_000;
        let mut s = Samples { high: 0, total: 0, reads: 0, rising: 0 };
        let mut last = p.cmt0.cmcnt().read().bits();
        let mut prev = pj1();
        while s.total < counts {
            let now = p.cmt0.cmcnt().read().bits();
            let d = now.wrapping_sub(last) as u64;
            last = now;
            s.total += d;
            if prev {
                s.high += d;
            }
            let h = pj1();
            s.reads += 1;
            if h && !prev {
                s.rising += 1;
            }
            prev = h;
        }
        s
    };

    // --- MTU6 at 1 kHz on PJ1
    let mut pwm = Pwm::mtu6(p.mtu6, pins.pj1, 1_000, &clocks);
    check(6, pwm.timing() == Timing { div: 2, period: 60_000, hz: 1_000 });
    check(7, pwm.max_duty_cycle() == 60_000);
    // MPC: PJ1PFS PSEL = 1 (MTIOC6A), PMR of PJ1
    check(8, rd8(0x0008_c1d1) == 1 && rd8(0x0008_c072) & 1 << 1 != 0);
    // TCR: clear on TGRA, PCLKA / 1 with TCR2 = PCLKA / 2; TMDR1: PWM
    // mode 1; TGRA = 59999; TSTRB.CST6
    let rd16 = |a| unsafe { read_volatile(a as *const u16) };
    check(9, rd8(0x000c_1a00) == 0x20 && rd8(0x000c_1a4c) == 1 && rd8(0x000c_1a02) == 2);
    check(10, rd16(0x000c_1a18) == 59_999 && rd8(0x000c_1a80) & 1 << 6 != 0);

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

    // --- 100 kHz (period 1200 counts at PCLKA / 1), 50 %: 1000 rising
    // edges in 10 ms
    let (mtu6, pj1_pin) = pwm.free();
    let mut pwm = Pwm::mtu6(mtu6, pj1_pin, 100_000, &clocks);
    check(19, pwm.timing() == Timing { div: 1, period: 1_200, hz: 100_000 });
    pwm.set_duty_cycle_percent(50).unwrap();
    wait(1);
    let s = sample(10_000);
    record(6, s.permille());
    record(7, s.rising);
    check(20, s.permille().abs_diff(500) <= 30);
    check(21, (990..=1010).contains(&s.rising));

    // Leave LD2 glowing dimly: 1 kHz, 5 %.
    let (mtu6, pj1_pin) = pwm.free();
    let mut pwm = Pwm::mtu6(mtu6, pj1_pin, 1_000, &clocks);
    pwm.set_duty_cycle_percent(5).unwrap();
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    hw_board::finish()
}
