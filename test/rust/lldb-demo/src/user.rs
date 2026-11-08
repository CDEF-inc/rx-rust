//! The end of lldb-demo: a loop in user mode (PSW.U = 1, on USP) that
//! keeps raising a software interrupt (#85). Stopped in the handler,
//! check-lldb.sh checks that LLDB unwinds into the loop with R0 = USP, from
//! the expression in `_rx_isr_common`'s CFI (`PSW.U ? saved USP : CFA`).

use core::arch::global_asm;
use core::sync::atomic::{AtomicU32, Ordering};

#[unsafe(no_mangle)]
static TRAPS: AtomicU32 = AtomicU32::new(0);

extern "C" fn trap() {
    TRAPS.fetch_add(1, Ordering::Relaxed);
}
rx_rt::interrupt!(64, trap);

// Its own stack, so that USP and ISP differ.
#[unsafe(no_mangle)]
static mut USER_STACK: [u32; 64] = [0; 64];

// Never returns. The return address stays on ISP, which DWARF cannot name,
// so the CFI ends the backtrace here (PC undefined).
global_asm!(
    "\t.pushsection .text.lldb_user_loop,\"ax\"",
    "\t.global _lldb_user_loop",
    "_lldb_user_loop:",
    "\t.cfi_startproc",
    "\t.cfi_undefined 17",
    "\tmov.l\t#_USER_STACK + 256, r1",
    "\tmvtc\tr1, usp",
    "\tsetpsw\tu",
    "1:",
    "\tint\t#64",
    "\tbra.b\t1b",
    "\t.cfi_endproc",
    "\t.popsection",
);

unsafe extern "C" {
    fn lldb_user_loop() -> !;
}

pub fn run() -> ! {
    unsafe { lldb_user_loop() }
}
