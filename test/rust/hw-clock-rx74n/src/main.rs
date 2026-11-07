//! rx74n-hal's clock set-up on the EK-RX74N board: PLL1 from the 24 MHz
//! crystal, CPUCLK0 480 MHz, or 600 MHz with `--cfg clock600`
//! (scripts/hw-build.sh builds both). The registers after the set-up, then
//! the clocks measured by the CAC (the main clock against the MOCO, PLL1P /
//! 16 and PCLKB against the main clock), `DelayNs` against CMT0 and the CPU
//! against CMT0. The LEDs blink in turn for two seconds of CMT time, so the
//! run takes about 2 s when the clocks are right. Booted from the code MRAM
//! by scripts/hw-run.sh; the results are left in `checks` / `failures` /
//! `first_failure` / `finished`.

#![no_std]
#![no_main]
// The result variables keep the names the test scripts read.
#![allow(non_upper_case_globals)]

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use rx74n_hal::clock::Config;
use rx74n_hal::delay::Delay;
use rx74n_hal::gpio::Parts;
use rx74n_hal::pac::Peripherals;
// The reset handler, vector tables and critical-section implementation.
use rx_rt as _;

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut first_failure: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;
// The CAC counts (main / MOCO, PLL1P / 16 and PCLKB against the main
// clock), CMT0 counts during a 100 ms delay and during a CPU loop.
#[unsafe(no_mangle)]
static mut measured: [u32; 5] = [0; 5];

// The configuration under test: its PLL1P (the dividers are the same in
// both), PLLCCR's PLLMUL and PLIDIV, and VSCR's VSCM.
#[cfg(not(clock600))]
const CONFIG: (Config, u32, u16, u8, u8) =
    (Config::pll_480mhz_from_24mhz(), 480_000_000, 39, 0, 0b010);
#[cfg(clock600)]
const CONFIG: (Config, u32, u16, u8, u8) =
    (Config::pll_600mhz_from_24mhz(), 600_000_000, 149, 2, 0b001);

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
    unsafe { write_volatile(&raw mut measured[i], v) };
}

// One CAC measurement: the count of the target clock (FMCS) during 8192
// cycles of the reference (RSCS). The first valid edge starts the counter,
// each later one ends a measurement; the second measurement is taken.
// Returns 0 if a measurement does not end.
macro_rules! cac_measure {
    ($cac:expr, $fmcs:expr, $rscs:expr) => {{
        let cac = &$cac;
        cac.cacr0().write(|w| w.cfme().clear_bit());
        while cac.cacr0().read().cfme().bit_is_set() {}
        cac.cacr1().write(|w| unsafe { w.fmcs().bits($fmcs) });
        cac.cacr2()
            .write(|w| unsafe { w.rps().set_bit().rscs().bits($rscs).rcds().bits(0b11) });
        cac.callvr().write(|w| unsafe { w.bits(0) });
        cac.caulvr().write(|w| unsafe { w.bits(0xffff) });
        cac.caicr()
            .write(|w| w.ferrfcl().set_bit().mendfcl().set_bit().ovffcl().set_bit());
        cac.cacr0().write(|w| w.cfme().set_bit());
        while cac.cacr0().read().cfme().bit_is_clear() {}
        let mut n = 0u32;
        for _ in 0..2 {
            let mut spins = 0u32;
            while cac.castr().read().mendf().bit_is_clear() && spins < 10_000_000 {
                spins += 1;
            }
            n = if spins < 10_000_000 { cac.cacntbr().read().bits() as u32 } else { 0 };
            cac.caicr().write(|w| w.mendfcl().set_bit());
        }
        cac.cacr0().write(|w| w.cfme().clear_bit());
        n
    }};
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();

    // --- the set-up and the registers it leaves
    let (config, pll1p, pllmul, plidiv, vscm) = CONFIG;
    let c = config.configure(&p.sysc, &p.sram, &p.mram);
    check(1, c.pll1p == pll1p && c.cpuclk == pll1p && c.iclk == pll1p / 2);
    check(2, c.mriclk == pll1p / 2 && c.mrpclk == pll1p / 4 && c.pclka == pll1p / 4);
    check(3, c.pclkb == pll1p / 8 && c.pclkd == pll1p / 4 && c.pclke == pll1p / 2);
    check(4, p.sysc.sckscr().read().cksel().bits() == 0b101);
    let osc = p.sysc.oscsf().read();
    check(5, osc.moscsf().bit_is_set() && osc.pllsf().bit_is_set());
    // MRPCK / ICK / PCKE / BCK / PCKA / PCKB / PCKC / PCKD = 4, 2, 2, 4, 4,
    // 8, 8, 4; MRICK / CPUCK0 = 2, 1.
    check(6, p.sysc.sckdivcr().read().bits() == 0x2112_2332);
    check(7, p.sysc.sckdivcr2().read().bits() == 0x1000);
    let pll = p.sysc.pllccr().read();
    check(8, pll.pllmul().bits() == pllmul && pll.plidiv().bits() == plidiv);
    check(9, p.sysc.pllccr2().read().plodivp().bits() == 1 && pll.plsrcsel().bit_is_clear());
    check(10, p.mram.mrcfreq().read().mrcmhz().bits() as u32 == pll1p / 2_000_000);
    check(11, p.mram.mrefreq().read().mremhz().bits() as u32 == pll1p / 4_000_000);
    check(12, p.sram.sramwtsc().read().wten().bit_is_set());
    // A crystal.
    check(13, p.sysc.momcr().read().mosel().bit_is_clear());
    // VSCR_2 (as after a reset) or VSCR_1, and PRCR_S protecting again.
    check(14, p.sysc.vscr().read().vscm().bits() == vscm);
    check(15, p.sysc.prcr_s().read().bits() & 0xff == 0);

    // --- the CAC (CAC0 to CAC2 out of module stop: MSTPCRC.MSTPC0)
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.mstpreg.mstpcrc().modify(|_, w| w.mstpc0().clear_bit());
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) });

    // The main clock against the MOCO (8 MHz, 7.2 MHz at least): 24 MHz
    // gives 24576 counts (23580 on this board: its MOCO runs 4% fast).
    let n = cac_measure!(p.cac0, 0b000, 0b011);
    record(0, n);
    check(16, (21_500..=27_500).contains(&n));
    // PLL1P / 16 against the main clock: 10240 counts at 480 MHz, 12800 at
    // 600, exactly but for the edge phases.
    let near = |n: u32, want: u32| want - 2 <= n && n <= want + 2;
    let n = cac_measure!(p.cac1, 0b001, 0b000);
    record(1, n);
    check(17, near(n, pll1p / 16 / 3_000 * 8192 / 8_000));
    // PCLKB against the main clock: 20480 counts at 60 MHz.
    let n = cac_measure!(p.cac1, 0b101, 0b000);
    record(2, n);
    check(18, near(n, pll1p / 8 / 3_000 * 8192 / 8_000));

    // --- the delay against CMT0 at PCLKB / 512 (117187.5 Hz at 60 MHz),
    // on CMT unit 0
    let cmt0_hz = c.pclkb / 512;
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.mstpreg.mstpcrf().modify(|_, w| w.mstpf15().clear_bit());
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) });
    p.cmt0.cmcor().write(|w| unsafe { w.bits(0xffff) });
    p.cmt0.cmcr().write(|w| unsafe { w.cks().bits(3) });
    p.cmt.cmstr0().write(|w| w.str0().set_bit());
    let mut delay = Delay::new(p.cmt2, &c);
    let a = p.cmt0.cmcnt().read().bits();
    delay.delay_ms(100);
    let n = p.cmt0.cmcnt().read().bits().wrapping_sub(a) as u32;
    record(3, n);
    check(19, (cmt0_hz / 10..cmt0_hz / 10 + 300).contains(&n));

    // The CPU against CMT0: a million iterations of a volatile load and a
    // branch take 37.5 ms at 480 MHz (about 18 cycles each) and 30 ms at
    // 600, and at least 125 ms on the MOCO (1 cycle each). The limit is
    // 50 ms.
    let x = 0u8;
    let a = p.cmt0.cmcnt().read().bits();
    for _ in 0..1_000_000 {
        unsafe { read_volatile(&x) };
    }
    let n = p.cmt0.cmcnt().read().bits().wrapping_sub(a) as u32;
    record(4, n);
    check(20, n < cmt0_hz / 20);

    // --- the LEDs in turn for 2 s
    let pins = Parts::take().unwrap();
    let mut led1 = pins.pf05.into_push_pull_output();
    let mut led2 = pins.p807.into_push_pull_output();
    let mut led3 = pins.p604.into_push_pull_output();
    for i in 0..30 {
        led1.set_state((i % 3 == 0).into()).unwrap();
        led2.set_state((i % 3 == 1).into()).unwrap();
        led3.set_state((i % 3 == 2).into()).unwrap();
        delay.delay_ms(66);
    }
    led1.set_low().unwrap();
    led2.set_low().unwrap();
    led3.set_low().unwrap();

    unsafe { write_volatile(&raw mut finished, 1) };
    done()
}

// The end of the test (the board runner's breakpoint).
#[unsafe(no_mangle)]
#[inline(never)]
extern "C" fn done() -> ! {
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe {
        failures += 1;
        first_failure = 999;
    }
    loop {}
}
