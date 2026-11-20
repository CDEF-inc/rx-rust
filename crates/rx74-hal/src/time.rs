//! An embassy-time driver (feature `embassy-time-driver`) on CMTW0 and
//! CMTW1, so that `embassy_time::Timer`, `Instant` and the async `Delay`
//! work.
//!
//! - **The time base:** CMTW0 counts PCLKB / 8 (7.5 MHz at PCLKB 60 MHz,
//!   9.375 MHz at 75 MHz) in 32 bits and wraps at FFFF FFFFh, with a compare
//!   match interrupt (CMWI0) at each wrap (every 9.5 or 7.6 minutes). The
//!   handler counts the wraps, which makes the count 64 bits.
//! - **The alarm:** CMTW1, at the same clock, is started for each alarm with
//!   CMWCOR set to the counts left and stopped by its interrupt (CMWI1). An
//!   alarm further away than 2^32 counts fires early and is set again for
//!   the rest.
//! - **Ticks:** the counts are converted to embassy-time's `TICK_HZ` (picked
//!   with its `tick-hz-*` features, 1 MHz by default) exactly, with the
//!   ratio reduced: at 7.5 MHz and 1 MHz a tick is 7.5 counts.
//!
//! The application calls [`init`] once and registers the two handlers with
//! [`bind_time_interrupts!`](crate::bind_time_interrupts). Until [`init`],
//! `now()` is 0. The timer queue is embassy-time-queue-utils' `Queue`
//! (integrated with embassy-executor by default).

use core::cell::{Cell, RefCell};
use core::ptr::{read_volatile, write_volatile};
use core::task::Waker;

use critical_section::{CriticalSection, Mutex};
use embassy_time_driver::{Driver, TICK_HZ};
use embassy_time_queue_utils::Queue;

use crate::clock::Clocks;
use crate::icu::{Interrupt, Priority};
use crate::mstp::{self, Mstpcr};
use crate::pac;

// The secure addresses, as the rest of the HAL.
const CMTW0: usize = 0x8782_6100;
const CMTW1: usize = 0x8782_6140;
// Register offsets (the same in both units).
const CMWSTR: usize = 0x00;
const CMWCR: usize = 0x04;
const CMWIOR: usize = 0x08;
const CMWCNT: usize = 0x10;
const CMWCOR: usize = 0x14;
/// CMWCR: CKS = 00 (PCLKB / 8), CMWIE, CMS = 0 (32 bits), CCLR = 000
/// (cleared by the CMWCOR compare match).
const CMWCR_RUN: u16 = 1 << 3;
/// CMWIOR: CMWE, without which there is no compare match at all (no
/// CMWI, no clearing); the input captures and output compares stay off.
const CMWIOR_CMWE: u16 = 1 << 15;

fn reg16(unit: usize, off: usize) -> *mut u16 {
    (unit + off) as _
}

fn reg32(unit: usize, off: usize) -> *mut u32 {
    (unit + off) as _
}

fn stop(unit: usize) {
    unsafe {
        write_volatile(reg16(unit, CMWSTR), 0);
        // Read back: stopped before the registers are set.
        let _ = read_volatile(reg16(unit, CMWSTR));
    }
}

fn start(unit: usize) {
    unsafe { write_volatile(reg16(unit, CMWSTR), 1) }
}

/// Counts to ticks: `ticks = counts * NUM / DEN`, the fraction reduced.
#[derive(Clone, Copy)]
struct Ratio {
    num: u64,
    den: u64,
}

impl Ratio {
    fn new(counts_hz: u64) -> Ratio {
        let (mut a, mut b) = (TICK_HZ, counts_hz);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Ratio { num: TICK_HZ / a, den: counts_hz / a }
    }

    /// The tick a count is in (rounded down).
    fn ticks(self, counts: u64) -> u64 {
        // num * den is at most TICK_HZ * counts_hz, which fits in 64 bits.
        counts / self.den * self.num + counts % self.den * self.num / self.den
    }

    /// The first count at or after tick `ticks`, saturating.
    fn counts(self, ticks: u64) -> u64 {
        (ticks / self.num)
            .saturating_mul(self.den)
            .saturating_add((ticks % self.num * self.den).div_ceil(self.num))
    }
}

struct State {
    /// The ratio; `None` until [`init`].
    ratio: Cell<Option<Ratio>>,
    /// CMTW0 wraps counted by its handler.
    wraps: Cell<u32>,
    queue: RefCell<Queue>,
}

struct RxTime {
    state: Mutex<State>,
}

impl RxTime {
    /// The 64-bit count of CMTW0.
    fn counts(&self, cs: CriticalSection) -> u64 {
        let s = self.state.borrow(cs);
        let mut wraps = s.wraps.get();
        let mut cnt = unsafe { read_volatile(reg32(CMTW0, CMWCNT)) };
        // A wrap whose handler has not run yet (interrupts are masked here):
        // count it, with the counter read after the wrap.
        if Interrupt::CMWI0.is_pending() {
            wraps += 1;
            cnt = unsafe { read_volatile(reg32(CMTW0, CMWCNT)) };
        }
        (wraps as u64) << 32 | cnt as u64
    }

    fn now_ticks(&self, cs: CriticalSection) -> u64 {
        match self.state.borrow(cs).ratio.get() {
            Some(r) => r.ticks(self.counts(cs)),
            None => 0,
        }
    }

    /// Starts CMTW1 to interrupt at tick `at`; false if `at` has passed.
    fn set_alarm(&self, cs: CriticalSection, at: u64) -> bool {
        stop(CMTW1);
        Interrupt::CMWI1.clear_pending();
        if at == u64::MAX {
            return true;
        }
        let Some(r) = self.state.borrow(cs).ratio.get() else {
            return true;
        };
        let target = r.counts(at);
        let now = self.counts(cs);
        if target <= now {
            return false;
        }
        // CMWI1 when CMWCNT reaches CMWCOR, counting from 0.
        let left = (target - now).min(u32::MAX as u64) as u32;
        unsafe {
            write_volatile(reg32(CMTW1, CMWCNT), 0);
            write_volatile(reg32(CMTW1, CMWCOR), left);
        }
        start(CMTW1);
        true
    }

    /// Wakes the expired timers and sets the alarm for the next one.
    fn run_queue(&self, cs: CriticalSection) {
        let mut queue = self.state.borrow(cs).queue.borrow_mut();
        let mut next = queue.next_expiration(self.now_ticks(cs));
        while !self.set_alarm(cs, next) {
            next = queue.next_expiration(self.now_ticks(cs));
        }
    }
}

impl Driver for RxTime {
    fn now(&self) -> u64 {
        critical_section::with(|cs| self.now_ticks(cs))
    }

    fn schedule_wake(&self, at: u64, waker: &Waker) {
        critical_section::with(|cs| {
            let changed = self.state.borrow(cs).queue.borrow_mut().schedule_wake(at, waker);
            if changed {
                self.run_queue(cs);
            }
        })
    }
}

embassy_time_driver::time_driver_impl!(static DRIVER: RxTime = RxTime {
    state: Mutex::new(State {
        ratio: Cell::new(None),
        wraps: Cell::new(0),
        queue: RefCell::new(Queue::new()),
    }),
});

/// Starts the time base on CMTW0 and the alarm on CMTW1 (taking CMTW
/// units 0 and 1 out of module stop), with their interrupts CMWI0 / CMWI1
/// at `priority`. Call it once, after the clocks are set (`clocks.pclkb`
/// must stay as it is), and register the handlers with
/// [`bind_time_interrupts!`](crate::bind_time_interrupts).
pub fn init(_cmtw0: pac::Cmtw0, _cmtw1: pac::Cmtw1, clocks: &Clocks, priority: Priority) {
    // MSTPCRF: MSTPF13 is CMTW unit 0, MSTPF12 unit 1.
    mstp::start(Mstpcr::F, 13);
    mstp::start(Mstpcr::F, 12);
    for unit in [CMTW0, CMTW1] {
        stop(unit);
        unsafe {
            write_volatile(reg16(unit, CMWCR), CMWCR_RUN);
            write_volatile(reg16(unit, CMWIOR), CMWIOR_CMWE);
            write_volatile(reg32(unit, CMWCNT), 0);
        }
    }
    unsafe { write_volatile(reg32(CMTW0, CMWCOR), u32::MAX) };
    for irq in [Interrupt::CMWI0, Interrupt::CMWI1] {
        irq.disable();
        irq.clear_pending();
        irq.set_priority(priority);
    }
    critical_section::with(|cs| {
        let s = DRIVER.state.borrow(cs);
        s.wraps.set(0);
        s.ratio.set(Some(Ratio::new(clocks.pclkb as u64 / 8)));
        start(CMTW0);
    });
    Interrupt::CMWI0.enable();
    Interrupt::CMWI1.enable();
}

/// The CMWI0 handler (vector 30): counts a wrap of CMTW0.
pub extern "C" fn on_cmwi0() {
    critical_section::with(|cs| {
        let w = &DRIVER.state.borrow(cs).wraps;
        w.set(w.get().wrapping_add(1));
    })
}

/// The CMWI1 handler (vector 31): stops the alarm, wakes the expired
/// timers and sets the next alarm.
pub extern "C" fn on_cmwi1() {
    critical_section::with(|cs| DRIVER.run_queue(cs))
}

/// Registers the time driver's handlers, [`time::on_cmwi0`](on_cmwi0) and
/// [`time::on_cmwi1`](on_cmwi1), for vectors 30 and 31 with rx-rt's
/// `interrupt!`. Use it once, in a crate that depends on rx-rt.
#[macro_export]
macro_rules! bind_time_interrupts {
    () => {
        rx_rt::interrupt!(30, $crate::time::on_cmwi0);
        rx_rt::interrupt!(31, $crate::time::on_cmwi1);
    };
}
