//! The HAL's embassy-time driver (`hal::time`, CMTW0 / CMTW1) with rx-rt's
//! executor on the board, at the boards' PLL clocks (AP-RX72M-0A: PCLKB
//! 60 MHz, CMTW 7.5 MHz; EK-RX74N / EK-RX74M: PCLKB 75 MHz, 9.375 MHz):
//!
//! - `Instant` against CMT2's `DelayNs` (another timer): 10 ms is 10 ms;
//! - `Timer::after` wakes the task on time (CMWI1), with the executor asleep
//!   in `wait` meanwhile;
//! - two tasks with timers wake in the order of their times;
//! - CMTW0 is put just before its wrap: `Instant` stays monotonic across it
//!   (CMWI0 counts it) and a timer over the wrap wakes on time.
//!
//! Booted from the code flash / MRAM by scripts/hw-run.sh (scripts/hw-build.sh
//! builds it); the results are left in hw-board's `checks` / `failures` /
//! `first_failure` / `finished`.

#![no_std]
#![no_main]

use core::mem::MaybeUninit;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use embassy_time::{Duration, Instant, Timer};
use embedded_hal::delay::DelayNs;
use hw_board::check;
use hw_board::hal::{self, delay::Delay, icu::Priority, pac::Peripherals};
use rx_rt::executor::{Executor, Spawner};

hal::bind_time_interrupts!();

#[cfg(feature = "ap-rx72m")]
const CMTW0: usize = 0x0009_4200;
#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
const CMTW0: usize = 0x8782_6100;
/// CMTW0's counts per second at the clocks below.
#[cfg(feature = "ap-rx72m")]
const CMTW_HZ: u32 = 7_500_000;
#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
const CMTW_HZ: u32 = 9_375_000;

#[cfg(feature = "ap-rx72m")]
fn setup(p: Peripherals) -> Delay {
    let clocks = hal::clock::Config::pll_240mhz_from_24mhz().configure(&p.system);
    hal::time::init(p.cmtw0, p.cmtw1, &clocks, Priority::new(5).unwrap());
    Delay::new(p.cmt2, &clocks)
}
#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
fn setup(p: Peripherals) -> Delay {
    let clocks = hal::clock::Config::pll_600mhz_from_24mhz().configure(&p.sysc, &p.sram, &p.mram);
    hal::time::init(p.cmtw0, p.cmtw1, &clocks, Priority::new(5).unwrap());
    Delay::new(p.cmt2, &clocks)
}

/// The sleepers in the order they woke, one decimal digit each.
static ORDER: AtomicU32 = AtomicU32::new(0);

#[embassy_executor::task(pool_size = 2)]
async fn sleeper(n: u32, ms: u64) {
    Timer::after_millis(ms).await;
    ORDER.store(ORDER.load(Relaxed) * 10 + n, Relaxed);
}

/// Microseconds from `a` to `b`.
fn us(a: Instant, b: Instant) -> u64 {
    (b - a).as_micros()
}

#[embassy_executor::task]
async fn run(spawner: Spawner, mut delay: Delay) {
    check(1, embassy_time::TICK_HZ == 1_000_000);

    // Time moves, and 10 ms of CMT2 is 10 ms of Instant (CMTW0).
    let a = Instant::now();
    delay.delay_ms(10);
    let b = Instant::now();
    check(2, b > a);
    let t = us(a, b);
    check(3, (10_000..10_100).contains(&t));

    // Timer::after: the task sleeps and CMWI1 wakes it on time.
    for (id, ms) in [(4, 1u64), (5, 100), (6, 1000)] {
        let a = Instant::now();
        Timer::after_millis(ms).await;
        let t = us(a, Instant::now());
        check(id, t >= ms * 1000 && t < ms * 1000 + 300);
    }

    // Two tasks: the 20 ms one wakes before the 40 ms one.
    spawner.spawn(sleeper(1, 40).unwrap());
    spawner.spawn(sleeper(2, 20).unwrap());
    Timer::after_millis(60).await;
    check(7, ORDER.load(Relaxed) == 21);

    // CMTW0 just before its wrap: 50 ms of counts left. Instant jumps ahead
    // (the counter moves forward) and stays monotonic over the wrap.
    let before = Instant::now();
    critical_section::with(|_| unsafe {
        write_volatile((CMTW0) as *mut u16, 0);
        write_volatile((CMTW0 + 0x10) as *mut u32, u32::MAX - CMTW_HZ / 20);
        write_volatile((CMTW0) as *mut u16, 1);
    });
    let mut last = Instant::now();
    check(8, last > before);
    let mut monotonic = true;
    let start = last;
    while us(start, last) < 150_000 {
        let now = Instant::now();
        monotonic &= now >= last;
        last = now;
    }
    check(9, monotonic);
    // The counter wrapped (it is small again) and CMWI0 counted it: the
    // 150 ms above passed on Instant, which needs the wrap.
    let cnt = unsafe { read_volatile((CMTW0 + 0x10) as *const u32) };
    check(10, cnt < CMTW_HZ);

    // A timer over the next wrap: 30 ms of counts left, a 100 ms timer.
    critical_section::with(|_| unsafe {
        write_volatile((CMTW0) as *mut u16, 0);
        write_volatile((CMTW0 + 0x10) as *mut u32, u32::MAX - CMTW_HZ / 33);
        write_volatile((CMTW0) as *mut u16, 1);
    });
    let a = Instant::now();
    Timer::after(Duration::from_millis(100)).await;
    let t = us(a, Instant::now());
    check(11, (100_000..100_300).contains(&t));
    let cnt = unsafe { read_volatile((CMTW0 + 0x10) as *const u32) };
    check(12, cnt < CMTW_HZ);

    hw_board::finish()
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let delay = setup(Peripherals::take().unwrap());
    static mut EXECUTOR: MaybeUninit<Executor> = MaybeUninit::uninit();
    let executor = unsafe { (&raw mut EXECUTOR).as_mut().unwrap().write(Executor::new()) };
    executor.run(|spawner| spawner.spawn(run(spawner, delay).unwrap()))
}
