//! The cause of the last reset (RSTSR0 / RSTSR1 / RSTSR3), the cold /
//! warm start flag (RSTSR2.CWSF) and the software reset (SWRR), at their
//! secure addresses.
//!
//! The flags stay set until they are cleared (or a power-on reset clears
//! them), so read [`cause`] early and [`clear`] them: the next reset then
//! reports only its own cause. A RES# pin reset sets none of them (nor
//! does the debugger's reset).

use core::ptr::{read_volatile, write_volatile};

const RSTSR0: *mut u8 = 0x8701_9a40usize as _;
const RSTSR1: *mut u32 = 0x8701_90c0usize as _;
const RSTSR2: *mut u8 = 0x8701_9a44usize as _;
const RSTSR3: *mut u8 = 0x8701_9a48usize as _;
const SWRR: *mut u16 = 0x8701_90b0usize as _;
const PRCR_S: *mut u16 = 0x8701_93fausize as _;

// RSTSR0: PORF, PVD0RF-PVD2RF (b1-b3), PVD4RF / PVD5RF (b5 / b6), DPSRSTF.
const PORF: u8 = 1 << 0;
const DPSRSTF: u8 = 1 << 7;
const RSTSR0_FLAGS: u8 = 0b1110_1111;
// RSTSR1: IWDTRF, WDT0RF, SWRF, LM0RF (b5), BUSRF (b10), CMRF (b14).
const IWDTRF: u32 = 1 << 0;
const WDT0RF: u32 = 1 << 1;
const SWRF: u32 = 1 << 2;
const RSTSR1_FLAGS: u32 = 1 << 14 | 1 << 10 | 1 << 5 | 0b111;
// RSTSR3: CVMRF (b0), TEMPRF (b7).
const RSTSR3_FLAGS: u8 = 1 << 7 | 1;

/// The reset flags, as read by [`cause`]. More than one can be set when
/// they were not cleared after an earlier reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cause {
    rstsr0: u8,
    rstsr1: u32,
    rstsr3: u8,
}

impl Cause {
    /// A power-on reset (RSTSR0.PORF).
    pub const fn power_on(self) -> bool {
        self.rstsr0 & PORF != 0
    }

    /// A voltage monitoring `n` reset (PVD0RF-PVD2RF, PVD4RF, PVD5RF): `n`
    /// 0, 1, 2, 4 or 5.
    pub const fn voltage_monitor(self, n: u8) -> bool {
        matches!(n, 0..=2 | 4 | 5) && self.rstsr0 & 1 << (n + 1) != 0
    }

    /// Release from deep software standby by an interrupt (DPSRSTF).
    pub const fn deep_standby(self) -> bool {
        self.rstsr0 & DPSRSTF != 0
    }

    /// An independent watchdog timer reset (RSTSR1.IWDTRF).
    pub const fn iwdt(self) -> bool {
        self.rstsr1 & IWDTRF != 0
    }

    /// A watchdog timer (WDT0) reset (WDT0RF).
    pub const fn wdt(self) -> bool {
        self.rstsr1 & WDT0RF != 0
    }

    /// A software reset (SWRF).
    pub const fn software(self) -> bool {
        self.rstsr1 & SWRF != 0
    }

    /// No flag set: a RES# pin reset (or the debugger's), or the flags were
    /// cleared.
    pub const fn pin(self) -> bool {
        self.rstsr0 & RSTSR0_FLAGS == 0
            && self.rstsr1 & RSTSR1_FLAGS == 0
            && self.rstsr3 & RSTSR3_FLAGS == 0
    }

    /// RSTSR0, RSTSR1 and RSTSR3 as read (for the flags without a method:
    /// RSTSR1's LM0RF, BUSRF and CMRF, RSTSR3's CVMRF and TEMPRF).
    pub const fn raw(self) -> (u8, u32, u8) {
        (self.rstsr0, self.rstsr1, self.rstsr3)
    }
}

/// The reset flags now.
pub fn cause() -> Cause {
    unsafe {
        Cause {
            rstsr0: read_volatile(RSTSR0),
            rstsr1: read_volatile(RSTSR1),
            rstsr3: read_volatile(RSTSR3),
        }
    }
}

/// Clears the reset flags (each is cleared by writing 0 after reading 1).
pub fn clear() {
    unsafe {
        let _ = read_volatile(RSTSR0);
        write_volatile(RSTSR0, 0);
        let _ = read_volatile(RSTSR1);
        write_volatile(RSTSR1, 0);
        let _ = read_volatile(RSTSR3);
        write_volatile(RSTSR3, 0);
    }
}

/// Whether RSTSR2.CWSF is set: [`set_warm_start`] was called since the last
/// power-on reset (a cold start clears it; other resets keep it).
pub fn is_warm_start() -> bool {
    unsafe { read_volatile(RSTSR2) & 1 != 0 }
}

/// Sets RSTSR2.CWSF, which only a power-on reset clears.
pub fn set_warm_start() {
    unsafe { write_volatile(RSTSR2, 1) }
}

/// Resets the MCU (SWRR = A501h, with PRCR_S.PRC1 set): RSTSR1.SWRF is set
/// after it.
pub fn software_reset() -> ! {
    unsafe { core::arch::asm!("clrpsw i") };
    unsafe {
        let prc = read_volatile(PRCR_S) & 0x00ff;
        write_volatile(PRCR_S, 0xa500 | prc | 1 << 1);
        write_volatile(SWRR, 0xa501);
    }
    loop {}
}
