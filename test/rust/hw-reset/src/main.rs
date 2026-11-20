//! The HAL's reset causes, software reset and watchdogs (`hal::reset`,
//! `hal::watchdog`) on the board, through real resets. The test runs four
//! times, told apart by the reset flags, and keeps its results in RAM that
//! rx-rt leaves alone (`.uninit`) until the last run reports them:
//!
//! 1. after the runner's reset (no flag): clear the flags, set the warm
//!    start flag, software reset;
//! 2. after the software reset (SWRF): start the IWDT, feed it for a
//!    while (no reset), then stop feeding;
//! 3. after the IWDT reset (IWDTRF): the same with the WDT;
//! 4. after the WDT reset (WDTRF): clear the flags, report.
//!
//! Booted from the code flash / MRAM by scripts/hw-run.sh (scripts/hw-build.sh
//! builds it); the results are left in hw-board's `checks` / `failures` /
//! `first_failure` / `finished`.

#![no_std]
#![no_main]

use core::ptr::{addr_of_mut, read_volatile, write_volatile};

use hw_board::check;
use hw_board::hal::pac::Peripherals;
use hw_board::hal::reset;
use hw_board::hal::watchdog::{Iwdt, IwdtConfig, IwdtDiv, Wdt, WdtConfig, WdtDiv};
// The reset handler and vector tables.
use rx_rt as _;

/// The IWDT's shortest timeout, and its counter's start value.
#[cfg(feature = "ap-rx72m")]
const IWDT: (IwdtConfig, u16) =
    (IwdtConfig::new(IwdtDiv::Div1, hw_board::hal::watchdog::Timeout::Cycles1024), 0x3ff);
#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
const IWDT: (IwdtConfig, u16) =
    (IwdtConfig::new(IwdtDiv::Div1, hw_board::hal::watchdog::IwdtTimeout::Cycles128), 0x7f);
/// The WDT at PCLKB / 4, 1024 cycles.
const WDT: (WdtConfig, u16) =
    (WdtConfig::new(WdtDiv::Div4, hw_board::hal::watchdog::Timeout::Cycles1024), 0x3ff);

const MAGIC: u32 = 0x5245_5345; // "RESE"


/// What survives the resets: the run, and each check's result (bit `id` of
/// `done` says it ran, of `ok` that it passed).
#[repr(C)]
struct State {
    magic: u32,
    run: u32,
    done: u64,
    ok: u64,
}

#[unsafe(link_section = ".uninit.hw_reset")]
static mut STATE: State = State { magic: 0, run: 0, done: 0, ok: 0 };

fn state() -> *mut State {
    addr_of_mut!(STATE)
}

fn record(id: u32, ok: bool) {
    unsafe {
        let s = state();
        write_volatile(&raw mut (*s).done, read_volatile(&raw const (*s).done) | 1 << id);
        if ok {
            write_volatile(&raw mut (*s).ok, read_volatile(&raw const (*s).ok) | 1 << id);
        }
    }
}

fn set_run(run: u32) {
    unsafe { write_volatile(&raw mut (*state()).run, run) }
}

/// The recorded checks as hw-board checks, then the end.
fn report() -> ! {
    let (done, ok) = unsafe { (read_volatile(&raw const (*state()).done), read_volatile(&raw const (*state()).ok)) };
    for id in 1..64 {
        if done & 1 << id != 0 {
            check(id, ok & 1 << id != 0);
        }
    }
    unsafe { write_volatile(&raw mut (*state()).magic, 0) };
    hw_board::finish()
}

/// Waits until `counter` is below half of `top`, at most a bounded spin;
/// whether it got there.
fn wait_half(top: u16, counter: impl Fn() -> u16) -> bool {
    let mut spins = 0u32;
    while counter() > top / 2 {
        spins += 1;
        if spins > 50_000_000 {
            return false;
        }
    }
    true
}

/// Waits for the watchdog's reset; returns if it does not come.
fn wait_for_reset() {
    let x = 0u32;
    for _ in 0..50_000_000u32 {
        unsafe { read_volatile(&x) };
    }
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let cause = reset::cause();
    let s = state();
    let (magic, run) = unsafe { (read_volatile(&raw const (*s).magic), read_volatile(&raw const (*s).run)) };
    let p = Peripherals::take().unwrap();

    if !cause.software() && !cause.iwdt() && !cause.wdt() {
        // 1. The runner's reset.
        unsafe { write_volatile(s, State { magic: MAGIC, run: 1, done: 0, ok: 0 }) };
        reset::clear();
        record(1, reset::cause().pin());
        reset::set_warm_start();
        record(2, reset::is_warm_start());
        reset::software_reset();
    }
    if magic != MAGIC {
        // A watchdog / software reset not from this test.
        record(63, false);
        report();
    }
    if cause.software() && run == 1 {
        // 2. After the software reset: SWRF alone, and still warm.
        record(3, !cause.iwdt() && !cause.wdt() && !cause.power_on());
        record(4, reset::is_warm_start());
        reset::clear();
        record(5, reset::cause().pin());
        let (config, top) = IWDT;
        let iwdt = Iwdt::start(p.iwdt, config);
        record(6, iwdt.is_ok());
        let Ok(mut iwdt) = iwdt else { report() };
        // Started from its timeout: the counter is at most its start value,
        // and it counts down (a counter stuck at 0 would pass the feeds).
        let first = iwdt.counter();
        record(7, first <= top && first > 0);
        record(17, wait_half(top, || iwdt.counter()) && iwdt.counter() < first);
        // Fed 20 times at half the timeout: no reset meanwhile.
        let mut fed_ok = true;
        for _ in 0..20 {
            fed_ok &= wait_half(top, || iwdt.counter());
            iwdt.feed();
        }
        record(8, fed_ok);
        set_run(2);
        wait_for_reset();
        record(9, false);
        report();
    }
    if cause.iwdt() && run == 2 {
        // 3. After the IWDT's reset.
        record(10, !cause.software() && !cause.wdt());
        reset::clear();
        #[cfg(feature = "ap-rx72m")]
        let wdt = Wdt::start(p.wdt, WDT.0);
        #[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
        let wdt = Wdt::start(p.wdt0, WDT.0);
        record(11, wdt.is_ok());
        let Ok(mut wdt) = wdt else { report() };
        let top = WDT.1;
        let first = wdt.counter();
        record(12, first <= top && first > 0);
        record(18, wait_half(top, || wdt.counter()) && wdt.counter() < first);
        let mut fed_ok = true;
        for _ in 0..20 {
            fed_ok &= wait_half(top, || wdt.counter());
            wdt.feed();
        }
        record(13, fed_ok);
        set_run(3);
        wait_for_reset();
        record(14, false);
        report();
    }
    if cause.wdt() && run == 3 {
        // 4. After the WDT's reset.
        record(15, !cause.software() && !cause.iwdt());
        reset::clear();
        record(16, reset::cause().pin());
        report();
    }
    record(62, false);
    report()
}
