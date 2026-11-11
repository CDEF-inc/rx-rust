//! rx74-hal on the EK-RX74N board: RSPIA0 as an SPI master on the Arduino
//! connector J13 (PA05 = RSPCK0-B = D13, PA06 = MOSI0-B = D11, PA07 =
//! MISO0-B = D12), with PLL1 at 480 MHz (PCLKA 120 MHz). The bit rate
//! choice, the pin functions, the internal loopbacks (SPPCR.SPLP2 / SPLP)
//! in the four SPI modes at 1 MHz and 30 MHz, the idle levels of RSPCK and
//! MOSI on the pins, and the transfer time at 1 MHz and 500 kHz (timed
//! with CMT0 at PCLKB / 8). If D11 and D12 are wired together, the bytes
//! also come back without the internal loopback (`loopback` = 1; without
//! the wire MISO reads FFh through its pull-up and those checks are
//! skipped). Booted from the code MRAM by scripts/hw-run.sh; the results
//! are in `checks` / `failures` / `first_failure` / `finished`.

#![no_std]
#![no_main]
// The result variables keep the names the test scripts read.
#![allow(non_upper_case_globals)]

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

use embedded_hal::spi::{MODE_0, MODE_1, MODE_2, MODE_3, Mode, SpiBus};
use rx74_hal::clock::{Clocks, Config};
use rx74_hal::gpio::Parts;
use rx74_hal::pac::Peripherals;
use rx74_hal::spi::{BitRate, Spi};

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
// CMT0 counts (PCLKB / 8 = 7.5 MHz) for 64 bytes at 1 MHz and 500 kHz.
#[unsafe(no_mangle)]
static mut spi_counts_1m: u32 = 0;
#[unsafe(no_mangle)]
static mut spi_counts_500k: u32 = 0;

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

fn rd32(a: usize) -> u32 {
    unsafe { read_volatile(a as *const u32) }
}

// PIDR of port A (16 bits): PA05 = RSPCK0, PA06 = MOSI0.
fn pa_pidr() -> u16 {
    unsafe { read_volatile(0x87c0_0144_usize as *const u16) }
}

fn mosi_high() -> bool {
    pa_pidr() & 1 << 6 != 0
}

fn sck_high() -> bool {
    pa_pidr() & 1 << 5 != 0
}

/// Every byte value through the loopback that `spi` is in, in place.
fn all_bytes_back(spi: &mut Spi, inverted: bool) -> bool {
    let mut buf = [0u8; 256];
    for (i, b) in buf.iter_mut().enumerate() {
        *b = i as u8;
    }
    spi.transfer_in_place(&mut buf).unwrap();
    buf.iter().enumerate().all(|(i, &b)| b == if inverted { !(i as u8) } else { i as u8 })
}

/// Sets RSPIA0 up again (after `free`) in `mode` at `hz`.
fn reopen(spi: Spi, mode: Mode, hz: u32, clocks: &Clocks) -> Spi {
    let (rspi, sck, mosi, miso) = spi.free();
    Spi::rspia0(rspi, sck, mosi, miso, mode, hz, clocks)
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = Config::pll_480mhz_from_24mhz().configure(&p.sysc, &p.sram, &p.mram);
    check(1, clocks.pclka == 120_000_000 && clocks.pclkb == 60_000_000);
    let pins = Parts::take().unwrap();

    // --- the bit rate choice: the highest rate not above the request,
    // PCLKA / (2 * (SPBR + 1) * 2^BRDV)
    let r = |hz| BitRate::new(120_000_000, hz);
    check(2, r(30_000_000) == Some(BitRate { spbr: 1, brdv: 0, hz: 30_000_000 }));
    check(3, r(1_000_000) == Some(BitRate { spbr: 59, brdv: 0, hz: 1_000_000 }));
    check(4, r(100_000) == Some(BitRate { spbr: 149, brdv: 2, hz: 100_000 }));
    check(5, r(29_297) == Some(BitRate { spbr: 255, brdv: 3, hz: 29_296 }));
    check(6, r(29_296).is_none() && r(0).is_none());

    // --- CMT0 at PCLKB / 8 = 7.5 MHz, free running, for the timing
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.mstpreg.mstpcrf().modify(|_, w| w.mstpf15().clear_bit());
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) });
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    p.cmt0.cmcr().write(|w| unsafe { w.cks().bits(0) });
    p.cmt0.cmcor().write(|w| unsafe { w.bits(0xffff) });
    p.cmt.cmstr0().write(|w| w.str0().set_bit());
    let cmcnt = || p.cmt0.cmcnt().read().bits();

    // --- RSPIA0 on PA05 / PA06 / PA07, mode 0 at 1 MHz
    let mut spi =
        Spi::rspia0(p.rspia0, pins.pa05, pins.pa06, pins.pa07, MODE_0, 1_000_000, &clocks);
    check(7, spi.bit_rate() == BitRate { spbr: 59, brdv: 0, hz: 1_000_000 });
    // PmnPFS of PA05-PA07: PSEL = 10h, PMR; PCR of PA07
    let pfs = |n: usize| rd32(0x87c0_0800 + 0x40 * 0x0a + 4 * n);
    check(8, (5..=7).all(|n| pfs(n) >> 24 & 0x7f == 0x10 && pfs(n) & 1 << 16 != 0));
    check(9, pfs(7) & 1 << 4 != 0);
    // SPCR: SYNDIS, MSTR, SPMS, SPE
    check(10, rd32(0x8745_c008) == 0xc100_0001);

    // Nothing wired to D12 (MISO) reads FFh through the pull-up, unless D11
    // (MOSI) is wired to it.
    let pattern = [0x55u8, 0xaa, 0x00, 0x0f];
    let mut got = [0u8; 4];
    spi.transfer(&mut got, &pattern).unwrap();
    if got == pattern {
        unsafe { write_volatile(&raw mut loopback, 1) };
        check(11, all_bytes_back(&mut spi, false));
    } else {
        check(11, got == [0xff; 4]);
    }

    // --- internal loopbacks: SPLP2 (as sent), SPLP (inverted)
    spi.set_loopback(Some(false));
    check(12, all_bytes_back(&mut spi, false));
    spi.set_loopback(Some(true));
    check(13, all_bytes_back(&mut spi, true));
    // read sends FFh; write drops what comes back
    let mut buf = [0u8; 3];
    spi.read(&mut buf).unwrap();
    check(14, buf == [0x00; 3]);
    spi.write(&[1, 2, 3]).unwrap();
    // transfer: the longer side sets the length
    let mut short = [0u8; 2];
    spi.transfer(&mut short, &[0x0f, 0xf0, 0x55]).unwrap();
    let mut long = [0u8; 3];
    spi.transfer(&mut long, &[0x3c]).unwrap();
    check(15, short == [0xf0, 0x0f] && long == [0xc3, 0x00, 0x00]);

    // MOSI keeps the last bit (the LSB) when idle; RSPCK idles low in mode 0.
    spi.set_loopback(Some(false));
    spi.write(&[0x01]).unwrap();
    spi.flush().unwrap();
    let high = mosi_high();
    spi.write(&[0xfe]).unwrap();
    spi.flush().unwrap();
    check(16, high && !mosi_high());
    check(17, !sck_high());

    // --- transfer time of 64 bytes: at least 8 bits each, and twice as
    // long at half the rate
    let time = |spi: &mut Spi| {
        let mut buf = [0u8; 64];
        let a = cmcnt();
        spi.transfer_in_place(&mut buf).unwrap();
        cmcnt().wrapping_sub(a) as u32
    };
    let t1 = time(&mut spi);
    let mut spi = reopen(spi, MODE_0, 500_000, &clocks);
    spi.set_loopback(Some(false));
    let t2 = time(&mut spi);
    unsafe {
        write_volatile(&raw mut spi_counts_1m, t1);
        write_volatile(&raw mut spi_counts_500k, t2);
    }
    // 64 * 8 bits at 1 MHz = 512 us = 3840 counts
    check(18, t1 >= 3840 && t1 < 3840 * 2);
    check(19, t2 * 10 >= t1 * 19 && t2 * 10 <= t1 * 21);

    // --- the other modes: RSPCK idles high with CPOL = 1, and the data
    // still comes back, also at 30 MHz
    let mut modes_ok = true;
    for (i, mode) in [MODE_1, MODE_2, MODE_3, MODE_0].into_iter().enumerate() {
        for hz in [1_000_000, 30_000_000] {
            spi = reopen(spi, mode, hz, &clocks);
            spi.set_loopback(Some(false));
            modes_ok &= all_bytes_back(&mut spi, false);
            spi.flush().unwrap();
            let cpol = matches!(i, 1 | 2);
            check(20 + i as u32, sck_high() == cpol);
        }
    }
    check(24, modes_ok);
    check(25, spi.bit_rate().hz == 30_000_000);

    let _ = spi.free();
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
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
