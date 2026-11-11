//! `DelayNs` on CMT2: the channel counts PCLKB / 8 continuously
//! (CMCOR = FFFFh) and a delay waits for the number of counts by reading
//! CMCNT. CMT0 / CMT1 stay free for the application.

use core::ptr::{read_volatile, write_volatile};

use embedded_hal::delay::DelayNs;

use crate::clock::Clocks;
use crate::mstp::{self, Mstpcr};
use crate::pac;

// CMSTR1 (STR2 = bit 0, STR3 = bit 1) is shared with CMT3.
const CMSTR1: *mut u16 = 0x0008_8010 as _;

/// A delay provider on CMT2.
pub struct Delay {
    cmt: pac::Cmt2,
    /// CMT2 counts per second (PCLKB / 8).
    hz: u64,
}

impl Delay {
    /// Starts CMT2 (taking CMT unit 1 out of module stop) at PCLKB / 8.
    pub fn new(cmt: pac::Cmt2, clocks: &Clocks) -> Self {
        mstp::start(Mstpcr::A, 14);
        critical_section::with(|_| unsafe {
            write_volatile(CMSTR1, read_volatile(CMSTR1) & !1);
        });
        cmt.cmcr().write(|w| unsafe { w.cks().bits(0) });
        cmt.cmcor().write(|w| unsafe { w.bits(0xffff) });
        cmt.cmcnt().write(|w| unsafe { w.bits(0) });
        critical_section::with(|_| unsafe {
            write_volatile(CMSTR1, read_volatile(CMSTR1) | 1);
        });
        Delay { cmt, hz: clocks.pclkb as u64 / 8 }
    }

    /// Stops CMT2 and gives it back.
    pub fn free(self) -> pac::Cmt2 {
        critical_section::with(|_| unsafe {
            write_volatile(CMSTR1, read_volatile(CMSTR1) & !1);
        });
        self.cmt
    }

    fn wait_counts(&mut self, mut counts: u64) {
        // The counter wraps at 10000h; read it often enough (more than once
        // per 8.7 ms at 7.5 MHz), which this loop does.
        let mut last = self.cmt.cmcnt().read().bits();
        while counts > 0 {
            let now = self.cmt.cmcnt().read().bits();
            let passed = now.wrapping_sub(last) as u64;
            last = now;
            counts = counts.saturating_sub(passed);
        }
    }
}

impl DelayNs for Delay {
    /// Waits at least `ns` nanoseconds, rounded up to whole counts (133 ns
    /// at PCLKB 60 MHz).
    fn delay_ns(&mut self, ns: u32) {
        let counts = (ns as u64 * self.hz).div_ceil(1_000_000_000);
        self.wait_counts(counts + 1);
    }

    fn delay_us(&mut self, us: u32) {
        let counts = (us as u64 * self.hz).div_ceil(1_000_000);
        self.wait_counts(counts + 1);
    }

    fn delay_ms(&mut self, ms: u32) {
        let counts = (ms as u64 * self.hz).div_ceil(1_000);
        self.wait_counts(counts + 1);
    }
}
