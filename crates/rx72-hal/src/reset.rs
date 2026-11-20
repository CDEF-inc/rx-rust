//! The cause of the last reset (RSTSR0 / RSTSR2), the cold / warm start
//! flag (RSTSR1.CWSF) and the software reset (SWRR).
//!
//! The flags stay set until they are cleared (or a power-on reset clears
//! them), so read [`cause`] early and [`clear`] them: the next reset then
//! reports only its own cause. A RES# pin reset sets none of them (nor
//! does the debugger's reset).

use core::ptr::{read_volatile, write_volatile};

const RSTSR0: *mut u8 = 0x0008_c290 as _;
const RSTSR1: *mut u8 = 0x0008_c291 as _;
const RSTSR2: *mut u8 = 0x0008_00c0 as _;
const SWRR: *mut u16 = 0x0008_00c2 as _;
const PRCR: *mut u16 = 0x0008_03fe as _;

// RSTSR0: PORF, LVD0RF-LVD2RF, DPSRSTF; RSTSR2: IWDTRF, WDTRF, SWRF.
const PORF: u8 = 1 << 0;
const DPSRSTF: u8 = 1 << 7;
const RSTSR0_FLAGS: u8 = 0b1000_1111;
const IWDTRF: u8 = 1 << 0;
const WDTRF: u8 = 1 << 1;
const SWRF: u8 = 1 << 2;
const RSTSR2_FLAGS: u8 = 0b111;

/// The reset flags, as read by [`cause`]. More than one can be set when
/// they were not cleared after an earlier reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cause {
    rstsr0: u8,
    rstsr2: u8,
}

impl Cause {
    /// A power-on reset (RSTSR0.PORF).
    pub const fn power_on(self) -> bool {
        self.rstsr0 & PORF != 0
    }

    /// A voltage monitoring `n` reset (LVD0RF-LVD2RF), `n` 0 to 2.
    pub const fn voltage_monitor(self, n: u8) -> bool {
        n < 3 && self.rstsr0 & 1 << (n + 1) != 0
    }

    /// Release from deep software standby by an interrupt (DPSRSTF).
    pub const fn deep_standby(self) -> bool {
        self.rstsr0 & DPSRSTF != 0
    }

    /// An independent watchdog timer reset (RSTSR2.IWDTRF).
    pub const fn iwdt(self) -> bool {
        self.rstsr2 & IWDTRF != 0
    }

    /// A watchdog timer reset (WDTRF).
    pub const fn wdt(self) -> bool {
        self.rstsr2 & WDTRF != 0
    }

    /// A software reset (SWRF).
    pub const fn software(self) -> bool {
        self.rstsr2 & SWRF != 0
    }

    /// No flag set: a RES# pin reset (or the debugger's), or the flags were
    /// cleared.
    pub const fn pin(self) -> bool {
        self.rstsr0 & RSTSR0_FLAGS == 0 && self.rstsr2 & RSTSR2_FLAGS == 0
    }

    /// RSTSR0 and RSTSR2 as read.
    pub const fn raw(self) -> (u8, u8) {
        (self.rstsr0, self.rstsr2)
    }
}

/// The reset flags now.
pub fn cause() -> Cause {
    unsafe { Cause { rstsr0: read_volatile(RSTSR0), rstsr2: read_volatile(RSTSR2) } }
}

/// Clears the reset flags (each is cleared by writing 0 after reading 1).
pub fn clear() {
    unsafe {
        let _ = read_volatile(RSTSR0);
        write_volatile(RSTSR0, 0);
        let _ = read_volatile(RSTSR2);
        write_volatile(RSTSR2, 0);
    }
}

/// Whether RSTSR1.CWSF is set: [`set_warm_start`] was called since the last
/// power-on reset (a cold start clears it; other resets keep it).
pub fn is_warm_start() -> bool {
    unsafe { read_volatile(RSTSR1) & 1 != 0 }
}

/// Sets RSTSR1.CWSF, which only a power-on reset clears.
pub fn set_warm_start() {
    unsafe { write_volatile(RSTSR1, 1) }
}

/// Resets the MCU (SWRR = A501h, with PRCR.PRC1 set): RSTSR2.SWRF is set
/// after it.
pub fn software_reset() -> ! {
    // Interrupts off, so that nothing runs between the protection and the
    // reset.
    unsafe { core::arch::asm!("clrpsw i") };
    unsafe {
        let prc = read_volatile(PRCR) & 0x000f;
        write_volatile(PRCR, 0xa500 | prc | 1 << 1);
        write_volatile(SWRR, 0xa501);
    }
    loop {}
}
