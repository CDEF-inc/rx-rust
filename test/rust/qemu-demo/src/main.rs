//! Rust-only program for QEMU's RX62N board, booted from the reset vector.
//!
//! Prints over SCI0, checks that rx-rt initialised .data and .bss, takes a
//! software interrupt (`int #64`) and counts CMT0 compare-match interrupts.
//! The results are left in `checks` / `failures` / `finished` for the
//! QEMU monitor.

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]
// The result variables keep the names the QEMU scripts read.
#![allow(non_upper_case_globals)]

use core::arch::asm;
use core::fmt::{self, Write};
use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, Ordering};

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut first_failure: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;

// One initialised and one zeroed variable, to check the startup code.
static mut DATA: u32 = 0x1234_5678;
static mut BSS: [u32; 4] = [0; 4];

static TICKS: AtomicU32 = AtomicU32::new(0);
static SOFT_IRQS: AtomicU32 = AtomicU32::new(0);

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

mod reg {
    // SCI0
    pub const SMR: *mut u8 = 0x0008_8240 as _;
    pub const BRR: *mut u8 = 0x0008_8241 as _;
    pub const SCR: *mut u8 = 0x0008_8242 as _;
    pub const TDR: *mut u8 = 0x0008_8243 as _;
    pub const SSR: *mut u8 = 0x0008_8244 as _;
    // CMT unit 0
    pub const CMSTR0: *mut u16 = 0x0008_8000 as _;
    pub const CMCR0: *mut u16 = 0x0008_8002 as _;
    pub const CMCOR0: *mut u16 = 0x0008_8006 as _;
    // ICU: IER03 bit 4 and IPR04 belong to vector 28 (CMI0).
    pub const IER03: *mut u8 = 0x0008_7203 as _;
    pub const IPR04: *mut u8 = 0x0008_7304 as _;
}

struct Sci0;

impl Sci0 {
    fn init() -> Self {
        unsafe {
            write_volatile(reg::SCR, 0);
            write_volatile(reg::SMR, 0); // 8N1, PCLK/1
            write_volatile(reg::BRR, 12);
            write_volatile(reg::SCR, 1 << 5); // TE
        }
        Sci0
    }

    fn put(&mut self, b: u8) {
        unsafe {
            while read_volatile(reg::SSR) & (1 << 7) == 0 {} // TDRE
            write_volatile(reg::TDR, b);
        }
    }
}

impl Write for Sci0 {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.put(b'\r');
            }
            self.put(b);
        }
        Ok(())
    }
}

// rx-none-elf has no compare-and-swap, so no fetch_add; a handler is not
// re-entered, and the main loop only reads.
extern "C" fn cmi0() {
    TICKS.store(TICKS.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
}
rx_rt::interrupt!(28, cmi0);

extern "C" fn soft() {
    SOFT_IRQS.store(SOFT_IRQS.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
}
rx_rt::interrupt!(64, soft);

#[unsafe(no_mangle)]
fn main() -> ! {
    let mut tx = Sci0::init();
    let _ = writeln!(tx, "Hello from Rust on RX!");

    unsafe {
        check(1, read_volatile(&raw const DATA) == 0x1234_5678);
        let bss = read_volatile(&raw const BSS);
        check(2, bss.iter().all(|&w| w == 0));
        write_volatile(&raw mut DATA, 7);
        check(3, read_volatile(&raw const DATA) == 7);
    }

    // A software interrupt goes through INTB even with PSW.I clear.
    unsafe { asm!("int #64") };
    unsafe { asm!("int #64") };
    check(4, SOFT_IRQS.load(Ordering::Relaxed) == 2);
    check(5, !rx_rt::interrupt::is_enabled());

    // CMT0: PCLK/8, compare match every 1000 counts, interrupt priority 1.
    unsafe {
        write_volatile(reg::CMCOR0, 1000);
        write_volatile(reg::CMCR0, 1 << 6); // CMIE, CKS = PCLK/8
        write_volatile(reg::IPR04, 1);
        write_volatile(reg::IER03, read_volatile(reg::IER03) | 1 << 4);
        write_volatile(reg::CMSTR0, 1);
    }
    rx_rt::interrupt::enable();
    check(6, rx_rt::interrupt::is_enabled());
    while TICKS.load(Ordering::Relaxed) < 5 {
        rx_rt::interrupt::wait();
    }
    let ticks = rx_rt::interrupt::free(|| TICKS.load(Ordering::Relaxed));
    check(7, ticks >= 5);
    check(8, rx_rt::interrupt::is_enabled());

    let (n, f) = unsafe { (checks, failures) };
    let _ = writeln!(tx, "ticks={ticks} soft={} checks={n} failures={f}", SOFT_IRQS.load(Ordering::Relaxed));
    unsafe { write_volatile(&raw mut finished, 1) };
    loop {
        rx_rt::interrupt::wait();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let _ = writeln!(Sci0, "panic: {}", info.message());
    unsafe {
        failures += 1;
        first_failure = 999;
    }
    loop {}
}
