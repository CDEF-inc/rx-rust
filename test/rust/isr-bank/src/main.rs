//! Checks rx-rt's register bank interrupt entry (`interrupt!(n, handler,
//! bank = b)`, RXv3 `save` / `rstr`): across a software interrupt the
//! interrupted code keeps R1-R5 and R7-R15, FPSW, A0 and A1 (72 bits),
//! DR0-DR3, DPSW, DCMR and USP, although the handler changes them; handlers start
//! with FPSW = DPSW = 0x100; an interrupt nested in a handler uses another
//! bank and leaves the handler's registers; and an entry of the first form
//! (`interrupt!(n, handler)`) works next to them. Built for RXv3:
//! RX_CPU=rxv3 (or rxv3e) scripts/run-rust-rom.sh test/rust/isr-bank. The
//! results are left in `checks` / `failures` / `finished` for the QEMU
//! monitor; scripts/hw-build.sh also builds it for the RX72M board, where
//! scripts/hw-run.sh stops it at `done`.

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]
// The result variables keep the names the QEMU scripts read.
#![allow(non_upper_case_globals)]

use core::arch::asm;
use core::panic::PanicInfo;
use core::ptr::write_volatile;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut first_failure: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;

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

// R1-R5 and R7-R15 (R6 can be the frame pointer).
type Regs = [u32; 14];

fn pattern(seed: u32) -> Regs {
    let mut r = [0; 14];
    for (i, x) in r.iter_mut().enumerate() {
        *x = seed ^ (i as u32).wrapping_mul(0x0101_0101);
    }
    r
}

// Raises interrupt V with the pattern of `seed` in R1-R5 and R7-R15 and
// returns the registers afterwards.
#[inline(never)]
fn across<const V: u32>(seed: u32) -> Regs {
    let mut r = pattern(seed);
    unsafe {
        asm!("int #{v}", v = const V,
             inout("r1") r[0], inout("r2") r[1], inout("r3") r[2],
             inout("r4") r[3], inout("r5") r[4], inout("r7") r[5],
             inout("r8") r[6], inout("r9") r[7], inout("r10") r[8],
             inout("r11") r[9], inout("r12") r[10], inout("r13") r[11],
             inout("r14") r[12], inout("r15") r[13])
    };
    r
}

fn fpsw() -> u32 {
    let v: u32;
    unsafe { asm!("mvfc fpsw, {}", out(reg) v, options(nomem, nostack)) };
    v
}

fn usp() -> u32 {
    let v: u32;
    unsafe { asm!("mvfc usp, {}", out(reg) v, options(nomem, nostack)) };
    v
}

fn set_usp(v: u32) {
    unsafe { asm!("mvtc {}, usp", in(reg) v, options(nomem, nostack)) };
}

// A0 and A1: guard (bits 95-64, sign-extended from bit 71), upper and lower
// words.
type Accs = [u32; 6];

fn accs() -> Accs {
    let r: [u32; 6];
    let (a, b, c, d, e, f);
    unsafe {
        asm!("mvfacgu #0, a0, {0}", "mvfachi #0, a0, {1}", "mvfaclo #0, a0, {2}",
             "mvfacgu #0, a1, {3}", "mvfachi #0, a1, {4}", "mvfaclo #0, a1, {5}",
             out(reg) a, out(reg) b, out(reg) c, out(reg) d, out(reg) e,
             out(reg) f, options(nomem, nostack))
    };
    r = [a, b, c, d, e, f];
    r
}

fn set_accs(v: Accs) {
    unsafe {
        asm!("mvtachi {1}, a0", "mvtacgu {0}, a0", "mvtaclo {2}, a0",
             "mvtachi {4}, a1", "mvtacgu {3}, a1", "mvtaclo {5}, a1",
             in(reg) v[0], in(reg) v[1], in(reg) v[2], in(reg) v[3],
             in(reg) v[4], in(reg) v[5], options(nomem, nostack))
    };
}

// DR0-DR3 as words (high, low), DPSW and DCMR.
type Dfpu = [u32; 10];

fn dfpu_state() -> Dfpu {
    let (h0, l0, h1, l1, dpsw, dcmr): (u32, u32, u32, u32, u32, u32);
    let (h2, l2, h3, l3): (u32, u32, u32, u32);
    unsafe {
        asm!("dmov.l drh0, {0}", "dmov.l drl0, {1}", "dmov.l drh1, {2}",
             "dmov.l drl1, {3}", "mvfdc dpsw, {4}", "mvfdc dcmr, {5}",
             out(reg) h0, out(reg) l0, out(reg) h1, out(reg) l1,
             out(reg) dpsw, out(reg) dcmr, options(nomem, nostack));
        asm!("dmov.l drh2, {0}", "dmov.l drl2, {1}", "dmov.l drh3, {2}",
             "dmov.l drl3, {3}", out(reg) h2, out(reg) l2, out(reg) h3,
             out(reg) l3, options(nomem, nostack))
    };
    [h0, l0, h1, l1, h2, l2, h3, l3, dpsw, dcmr]
}

fn set_dfpu_state(s: Dfpu) {
    unsafe {
        asm!("dmov.l {0}, drh0", "dmov.l {1}, drl0", "dmov.l {2}, drh1",
             "dmov.l {3}, drl1", "mvtdc {4}, dpsw", "mvtdc {5}, dcmr",
             in(reg) s[0], in(reg) s[1], in(reg) s[2], in(reg) s[3],
             in(reg) s[8], in(reg) s[9], options(nomem, nostack));
        asm!("dmov.l {0}, drh2", "dmov.l {1}, drl2", "dmov.l {2}, drh3",
             "dmov.l {3}, drl3", in(reg) s[4], in(reg) s[5], in(reg) s[6],
             in(reg) s[7], options(nomem, nostack))
    };
}

// What a handler does to the state: FPSW (rounding towards -infinity), the
// accumulators, the DFPU registers, USP, and R1-R5, R14 and R15 (the
// registers a handler may change; it keeps R6-R13 itself).
fn clobber(k: u32) {
    unsafe { asm!("mvtc #3, fpsw", options(nomem, nostack)) };
    set_accs([0x0000_0080 ^ k, 0xdead_beef, 0x0bad_f00d, 0xffff_ff80, 0xfeed_face ^ k,
              0xcafe_babe]);
    set_dfpu_state([0xdead, 0xbeef ^ k, 0xbad, 0xf00d, 0xd2, 0xd2 ^ k, 0xd3, 0xd3, 0x103,
                    0]);
    set_usp(0x1234_0000 ^ k);
    unsafe {
        asm!("mov.l #0xbad00001, r1", "mov.l #0xbad00002, r2", "mov.l #0xbad00003, r3",
             "mov.l #0xbad00004, r4", "mov.l #0xbad00005, r5",
             "mov.l #0xbad0000e, r14", "mov.l #0xbad0000f, r15",
             out("r1") _, out("r2") _, out("r3") _, out("r4") _, out("r5") _,
             out("r14") _, out("r15") _, options(nomem, nostack))
    };
}

static OUTER: AtomicU32 = AtomicU32::new(0);
static INNER: AtomicU32 = AtomicU32::new(0);
static PLAIN: AtomicU32 = AtomicU32::new(0);
static ENTRY_FPSW: AtomicU32 = AtomicU32::new(0);
static ENTRY_DPSW: AtomicU32 = AtomicU32::new(0);
static INNER_FPSW: AtomicU32 = AtomicU32::new(0);
static INNER_DPSW: AtomicU32 = AtomicU32::new(0);
static NESTED_REGS: AtomicBool = AtomicBool::new(false);
static NESTED_STATE: AtomicBool = AtomicBool::new(false);

// Bank 1. Raises interrupt 66 (bank 2) with its own registers and state.
extern "C" fn outer() {
    ENTRY_FPSW.store(fpsw(), Ordering::Relaxed);
    ENTRY_DPSW.store(dfpu_state()[8], Ordering::Relaxed);
    let n = OUTER.load(Ordering::Relaxed);
    unsafe { asm!("mvtc #0x102, fpsw", options(nomem, nostack)) };
    let a = [1, 2, 3 + n, 0xffff_ffff, 5, 6];
    set_accs(a);
    let d = [0x4008_0000, n, 0x3ff8_0000, 7, 0x4010_0000, n + 2, 0x3fe0_0000, 9, 0x102, 1];
    set_dfpu_state(d);
    set_usp(0x5678_0000 + n);
    let seed = 0xa5a5_0000 + n;
    NESTED_REGS.store(across::<66>(seed) == pattern(seed), Ordering::Relaxed);
    NESTED_STATE.store(
        fpsw() == 0x102 && accs() == a && dfpu_state() == d && usp() == 0x5678_0000 + n,
        Ordering::Relaxed,
    );
    clobber(1);
    OUTER.store(n + 1, Ordering::Relaxed);
}
rx_rt::interrupt!(65, outer, bank = 1);

// Bank 2, nested in `outer`.
extern "C" fn inner() {
    INNER_FPSW.store(fpsw(), Ordering::Relaxed);
    INNER_DPSW.store(dfpu_state()[8], Ordering::Relaxed);
    clobber(2);
    INNER.store(INNER.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
}
rx_rt::interrupt!(66, inner, bank = 2);

// The first form, next to the bank entries. It does not save the
// accumulators (no `save-acc`) or USP, so this handler leaves them.
extern "C" fn plain() {
    unsafe {
        asm!("mvtc #3, fpsw", "mov.l #0xbad00001, r1", "mov.l #0xbad00005, r5",
             "mov.l #0xbad0000f, r15", out("r1") _, out("r5") _, out("r15") _,
             options(nomem, nostack))
    };
    PLAIN.store(PLAIN.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
}
rx_rt::interrupt!(67, plain);

#[unsafe(no_mangle)]
fn main() -> ! {
    for round in 0..3u32 {
        // Rounding towards zero in the interrupted code. (MVTC cannot set
        // the cause flags, so they are not used.)
        unsafe { asm!("mvtc #0x101, fpsw", options(nomem, nostack)) };
        let a = [0x0000_0011 + round, 0x2468_ace0 ^ round, 0x1357_9bdf,
                 0xffff_ff9a, 0x8000_0001, 0x7fff_fffe ^ round];
        set_accs(a);
        let d = [0x3ff0_0000 + round, 0x1111_1111, 0x4000_0000,
                 0x2222_2222 ^ round, 0x4020_0000, 0x3333_3333 ^ round,
                 0xc000_0000 + round, 0x4444_4444, 0x101, 1];
        set_dfpu_state(d);
        let u = 0x0000_4000 + 16 * round;
        set_usp(u);
        let id = 10 * (round + 1);
        let seed = 0x1000_0000 * (round + 1);
        check(id, across::<65>(seed) == pattern(seed));
        check(id + 1, OUTER.load(Ordering::Relaxed) == round + 1);
        check(id + 2, INNER.load(Ordering::Relaxed) == round + 1);
        check(id + 3, fpsw() == 0x101);
        check(id + 4, accs() == a);
        check(id + 5, dfpu_state() == d);
        check(id + 6, usp() == u);
        check(id + 7, ENTRY_FPSW.load(Ordering::Relaxed) == 0x100
                      && ENTRY_DPSW.load(Ordering::Relaxed) == 0x100);
        check(id + 8, INNER_FPSW.load(Ordering::Relaxed) == 0x100
                      && INNER_DPSW.load(Ordering::Relaxed) == 0x100);
        check(id + 9, NESTED_REGS.load(Ordering::Relaxed)
                      && NESTED_STATE.load(Ordering::Relaxed));
    }

    unsafe { asm!("mvtc #0x101, fpsw", options(nomem, nostack)) };
    check(40, across::<67>(0x6000_0000) == pattern(0x6000_0000));
    check(41, PLAIN.load(Ordering::Relaxed) == 1);
    check(42, fpsw() == 0x101);

    unsafe { write_volatile(&raw mut finished, 1) };
    done()
}

// The end of the test (the board runner's breakpoint).
#[unsafe(no_mangle)]
#[inline(never)]
extern "C" fn done() -> ! {
    loop {
        rx_rt::interrupt::wait();
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe {
        failures += 1;
        first_failure = 999;
    }
    loop {}
}
