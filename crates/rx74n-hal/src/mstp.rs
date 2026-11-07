//! Module stop control: the drivers take their module out of module stop
//! (MSTPCRB / MSTPCRF, write-protected by PRCR_S.PRC1).

use core::ptr::{read_volatile, write_volatile};

const PRCR_S: *mut u16 = 0x8701_93fa_usize as _;

/// The module stop control registers.
#[derive(Clone, Copy)]
#[repr(usize)]
pub(crate) enum Mstpcr {
    B = 0x8780_3004,
    F = 0x8780_3014,
}

/// Clears bit `bit` of the register (starts the module), with PRC1 set
/// for the write and PRCR_S restored afterwards.
pub(crate) fn start(reg: Mstpcr, bit: u32) {
    critical_section::with(|_| unsafe {
        let prc = read_volatile(PRCR_S) & 0x00ff;
        write_volatile(PRCR_S, 0xa500 | prc | 1 << 1);
        let r = reg as usize as *mut u32;
        write_volatile(r, read_volatile(r) & !(1 << bit));
        // A read back so that the module runs before it is used.
        let _ = read_volatile(r);
        write_volatile(PRCR_S, 0xa500 | prc);
    });
}
