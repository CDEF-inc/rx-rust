//! rx72m-hal on the AP-RX72M-0A board: DelayNs on CMT2 (timed with CMT0
//! on the same PCLKB / 8), the MPC pin functions, and SCI2 as a UART on CN8
//! (P13 = TXD2, P12 = RXD2) at 115200 bps. It sends a line, checks the TXD
//! pin goes low while sending, and, if CN8's RXD and TXD (pins 1 and 2)
//! are wired together, that the bytes come back (`loopback` = 1; without
//! the wire those checks are skipped). Booted from the code flash by
//! scripts/hw-run.sh; the results are in `checks` / `failures` /
//! `first_failure` / `finished`.

#![no_std]
#![no_main]
// The result variables keep the names the test scripts read.
#![allow(non_upper_case_globals)]

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

use embedded_hal::delay::DelayNs;
use embedded_io::{Read, ReadReady, Write};
use rx72m_hal::clock::Config;
use rx72m_hal::delay::Delay;
use rx72m_hal::gpio::Parts;
use rx72m_hal::pac::Peripherals;
use rx72m_hal::serial::{BaudSettings, Serial};

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut first_failure: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;
#[unsafe(no_mangle)]
static mut loopback: u32 = 0;

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

fn rd8(a: usize) -> u8 {
    unsafe { read_volatile(a as *const u8) }
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = Config::pll_240mhz_from_24mhz().configure(&p.system);
    let pins = Parts::take().unwrap();

    // --- DelayNs on CMT2, timed with CMT0 (both PCLKB / 8 = 7.5 MHz)
    let mut delay = Delay::new(p.cmt2, &clocks);
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.system.mstpcra().modify(|_, w| w.mstpa15().clear_bit());
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) });
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    p.cmt0.cmcr().write(|w| unsafe { w.cks().bits(0) });
    p.cmt0.cmcor().write(|w| unsafe { w.bits(0xffff) });
    p.cmt.cmstr0().write(|w| w.str0().set_bit());
    let mut timed = |f: &mut dyn FnMut(&mut Delay)| {
        let a = p.cmt0.cmcnt().read().bits();
        f(&mut delay);
        p.cmt0.cmcnt().read().bits().wrapping_sub(a) as u32
    };
    let ms5 = timed(&mut |d| d.delay_ms(5)); // 37500 counts
    check(1, (37_500..37_800).contains(&ms5));
    let us100 = timed(&mut |d| d.delay_us(100)); // 750 counts
    check(2, (750..800).contains(&us100));
    let ns1000 = timed(&mut |d| d.delay_ns(1000)); // 7.5 counts
    check(3, (8..40).contains(&ns1000));
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());

    // --- the baud rate settings: as close as an exhaustive search over
    // k (32, 16, 8), CKS and BRR, for PCLKB 60 MHz
    let mut baud_ok = true;
    for baud in [
        9_600u32, 19_200, 38_400, 57_600, 115_200, 230_400, 250_000, 454_546, 460_800,
        921_600, 1_000_000, 2_000_000,
    ] {
        let mut best = u64::MAX;
        for k in [32u64, 16, 8] {
            for cks in 0..4 {
                for n in 1..=256u64 {
                    let actual = 60_000_000 / ((k << (2 * cks)) * n);
                    best = best.min(actual.abs_diff(baud as u64));
                }
            }
        }
        let s = BaudSettings::new(60_000_000, baud).unwrap();
        baud_ok &= (s.baud as u64).abs_diff(baud as u64) == best;
    }
    check(15, baud_ok);
    // 454546 bps: ABCS = BGDM = 1, BRR = 16, 441176 bps (within 3%)
    let s = BaudSettings::new(60_000_000, 454_546).unwrap();
    check(16, s.baud == 441_176 && s.brr == 16 && s.abcs && s.bgdm);

    // --- SCI2 on P13 / P12
    let mut serial = Serial::sci2(p.sci2, pins.p13, pins.p12, 115_200, &clocks);
    let s = serial.settings();
    check(4, s.baud == 115_384 && s.brr == 64 && s.abcs && s.bgdm);
    // MPC: P12PFS / P13PFS PSEL = 0Ah, PMR of P12 / P13 set
    check(5, rd8(0x0008_c14a) & 0x3f == 0x0a && rd8(0x0008_c14b) & 0x3f == 0x0a);
    check(6, rd8(0x0008_c061) & 0b1100 == 0b1100);
    // TE with TIE: TXI2 (IR063) at once, nothing received (IR062)
    check(7, serial.tx_ready() && !serial.rx_ready());

    // A line for whoever listens on CN8.
    serial.write_all(b"Hello from rx72m-hal on SCI2\r\n").unwrap();
    serial.flush().unwrap();
    check(8, rd8(0x0008_a044) & 1 << 2 != 0); // SCI2 SSR.TEND

    // TXD goes low while sending zero bytes (PIDR of P13), high when idle.
    let p13 = || rd8(0x0008_c041) & 1 << 3 != 0;
    check(9, p13());
    // Drop whatever the zeros bring back if CN8 is wired, before the
    // loopback test.
    let mut lows = 0u32;
    for _ in 0..8 {
        serial.write_all(&[0]).unwrap();
        for _ in 0..200 {
            if !p13() {
                lows += 1;
            }
        }
    }
    serial.flush().unwrap();
    check(10, lows > 0);
    check(11, p13());
    let mut sink = [0u8; 16];
    for _ in 0..100_000 {
        if serial.read_ready().unwrap() {
            let _ = serial.read(&mut sink);
        }
    }

    // --- loopback, if CN8's RXD and TXD are wired
    let out = [0x55u8, 0xaa, 0x00, 0xff, b'R', b'X'];
    let mut got = [0u8; 6];
    let mut n = 0;
    for &b in &out {
        serial.write_all(&[b]).unwrap();
        let mut t = 0u32;
        while !serial.read_ready().unwrap() && t < 200_000 {
            t += 1;
        }
        if serial.read_ready().unwrap() {
            if let Ok(k) = serial.read(&mut got[n..n + 1]) {
                n += k;
            }
        }
    }
    serial.flush().unwrap();
    if n > 0 {
        unsafe { write_volatile(&raw mut loopback, 1) };
        check(12, n == out.len() && got == out);
    }
    // No receive errors: RXD2 has the pull-up, so an open CN8 idles high.
    check(13, rd8(0x0008_a044) & 0x38 == 0); // SSR ORER / FER / PER
    check(14, rd8(0x0008_c0c1) & 1 << 2 != 0); // PCR of P12

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
