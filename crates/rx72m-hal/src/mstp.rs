//! Module stop control: the drivers take their module out of module stop
//! (MSTPCRA / MSTPCRB, write-protected by PRCR.PRC1).

use core::ptr::{read_volatile, write_volatile};

const PRCR: *mut u16 = 0x0008_03fe as _;

/// The module stop control registers.
#[derive(Clone, Copy)]
pub(crate) enum Mstpcr {
    A = 0x0008_0010,
    B = 0x0008_0014,
}

/// Clears bit `bit` of the register (starts the module), with PRC1 set
/// for the write and PRCR restored afterwards.
pub(crate) fn start(reg: Mstpcr, bit: u32) {
    critical_section::with(|_| unsafe {
        let prc = read_volatile(PRCR) & 0x000f;
        write_volatile(PRCR, 0xa500 | prc | 1 << 1);
        let r = reg as usize as *mut u32;
        write_volatile(r, read_volatile(r) & !(1 << bit));
        // A read back so that the module runs before it is used.
        let _ = read_volatile(r);
        write_volatile(PRCR, 0xa500 | prc);
    });
}
