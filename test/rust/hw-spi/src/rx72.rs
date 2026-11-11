//! rx72-hal on the AP-RX72M-0A board: RSPI2 as an SPI master on CN9
//! (P56 = RSPCKC, P54 = MOSIC, P55 = MISOC). The bit rate choice, the MPC
//! pin functions, the internal loopbacks (SPPCR.SPLP2 / SPLP) in the four
//! SPI modes at 1 MHz and 30 MHz, the idle levels of RSPCK and MOSI on the
//! pins, and the transfer time at 1 MHz and 500 kHz (timed with CMT0 at
//! PCLKB / 8). If CN9's MOSI and MISO (pins 3 and 4) are wired together,
//! the bytes also come back without the internal loopback (`loopback` = 1;
//! without the wire MISO reads FFh through its pull-up and those checks
//! are skipped). Booted from the code flash by scripts/hw-run.sh; the
//! results are in `checks` / `failures` / `first_failure` / `finished`.

use core::ptr::{read_volatile, write_volatile};

use embedded_hal::spi::{MODE_0, MODE_1, MODE_2, MODE_3, Mode, SpiBus};
use hw_board::hal::clock::{Clocks, Config};
use hw_board::hal::gpio::Parts;
use hw_board::hal::pac::{Peripherals, Rspi2};
use hw_board::hal::spi::{BitRate, Spi};
use hw_board::check;

#[unsafe(no_mangle)]
static mut loopback: u32 = 0;
// CMT0 counts (PCLKB / 8 = 7.5 MHz) for 64 bytes at 1 MHz and 500 kHz.
#[unsafe(no_mangle)]
static mut spi_counts_1m: u32 = 0;
#[unsafe(no_mangle)]
static mut spi_counts_500k: u32 = 0;

fn rd8(a: usize) -> u8 {
    unsafe { read_volatile(a as *const u8) }
}

// PIDR of port 5: P54 = MOSIC, P56 = RSPCKC.
fn mosi_high() -> bool {
    rd8(0x0008_c045) & 1 << 4 != 0
}

fn sck_high() -> bool {
    rd8(0x0008_c045) & 1 << 6 != 0
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

/// Sets RSPI2 up again (after `free`) in `mode` at `hz`.
fn reopen(spi: Spi, mode: Mode, hz: u32, clocks: &Clocks) -> Spi {
    let (rspi, sck, mosi, miso): (Rspi2, _, _, _) = spi.free();
    Spi::rspi2(rspi, sck, mosi, miso, mode, hz, clocks)
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = Config::pll_240mhz_from_24mhz().configure(&p.system);
    let pins = Parts::take().unwrap();

    // --- the bit rate choice: the highest rate not above the request,
    // PCLKA / (2 * (SPBR + 1) * 2^BRDV), PCLKA 120 MHz
    let r = |hz| BitRate::new(120_000_000, hz);
    check(1, r(30_000_000) == Some(BitRate { spbr: 1, brdv: 0, hz: 30_000_000 }));
    check(2, r(1_000_000) == Some(BitRate { spbr: 59, brdv: 0, hz: 1_000_000 }));
    check(3, r(40_000_000) == Some(BitRate { spbr: 1, brdv: 0, hz: 30_000_000 }));
    check(4, r(100_000_000) == Some(BitRate { spbr: 0, brdv: 0, hz: 60_000_000 }));
    // 100 kHz needs a divisor of 1200: BRDV = 2 (8 * 150)
    check(5, r(100_000) == Some(BitRate { spbr: 149, brdv: 2, hz: 100_000 }));
    // The slowest is PCLKA / 4096 = 29296.875 Hz
    check(6, r(29_297) == Some(BitRate { spbr: 255, brdv: 3, hz: 29_296 }));
    check(7, r(29_296).is_none() && r(0).is_none());

    // --- CMT0 at PCLKB / 8 for the timing
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.system.mstpcra().modify(|_, w| w.mstpa15().clear_bit());
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) });
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    p.cmt0.cmcr().write(|w| unsafe { w.cks().bits(0) });
    p.cmt0.cmcor().write(|w| unsafe { w.bits(0xffff) });
    p.cmt.cmstr0().write(|w| w.str0().set_bit());
    let cmcnt = || p.cmt0.cmcnt().read().bits();

    // --- RSPI2 on P56 / P54 / P55, mode 0 at 1 MHz
    let mut spi = Spi::rspi2(p.rspi2, pins.p56, pins.p54, pins.p55, MODE_0, 1_000_000, &clocks);
    check(8, spi.bit_rate() == BitRate { spbr: 59, brdv: 0, hz: 1_000_000 });
    // MPC: P54PFS / P55PFS / P56PFS PSEL = 0Dh, PMR of P54-P56, PCR of P55
    let psel = |a| rd8(a) & 0x3f == 0x0d;
    check(9, psel(0x0008_c16c) && psel(0x0008_c16d) && psel(0x0008_c16e));
    check(10, rd8(0x0008_c065) & 0x70 == 0x70);
    check(11, rd8(0x0008_c0c5) & 1 << 5 != 0);
    // SPCR: SPE, MSTR, SPMS (clock synchronous master, full duplex)
    check(12, rd8(0x000d_0300) == 0x49);

    // Nothing wired to CN9's MISO reads FFh through the pull-up, unless
    // MOSI is wired to it.
    let pattern = [0x55u8, 0xaa, 0x00, 0x0f];
    let mut got = [0u8; 4];
    spi.transfer(&mut got, &pattern).unwrap();
    if got == pattern {
        unsafe { write_volatile(&raw mut loopback, 1) };
        check(13, all_bytes_back(&mut spi, false));
    } else {
        check(13, got == [0xff; 4]);
    }

    // --- internal loopbacks: SPLP2 (as sent), SPLP (inverted)
    spi.set_loopback(Some(false));
    check(14, all_bytes_back(&mut spi, false));
    spi.set_loopback(Some(true));
    check(15, all_bytes_back(&mut spi, true));
    // read sends FFh; write drops what comes back
    let mut buf = [0u8; 3];
    spi.read(&mut buf).unwrap();
    check(16, buf == [0x00; 3]);
    spi.write(&[1, 2, 3]).unwrap();
    // transfer: the longer side sets the length
    let mut short = [0u8; 2];
    spi.transfer(&mut short, &[0x0f, 0xf0, 0x55]).unwrap();
    let mut long = [0u8; 3];
    spi.transfer(&mut long, &[0x3c]).unwrap();
    check(17, short == [0xf0, 0x0f] && long == [0xc3, 0x00, 0x00]);

    // MOSI keeps the last bit (the LSB) when idle; RSPCK idles low in mode 0.
    spi.set_loopback(Some(false));
    spi.write(&[0x01]).unwrap();
    spi.flush().unwrap();
    let high = mosi_high();
    spi.write(&[0xfe]).unwrap();
    spi.flush().unwrap();
    check(18, high && !mosi_high());
    check(19, !sck_high());

    // --- transfer time of 64 bytes: at least 8 bits each, and twice as
    // long at half the rate (about 11 RSPCK per byte with the delays)
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
    check(20, t1 >= 3840 && t1 < 3840 * 2);
    check(21, t2 * 10 >= t1 * 19 && t2 * 10 <= t1 * 21);

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
            check(22 + i as u32, sck_high() == cpol);
        }
    }
    check(26, modes_ok);
    check(27, spi.bit_rate().hz == 30_000_000);

    let _ = spi.free();
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    hw_board::finish()
}
