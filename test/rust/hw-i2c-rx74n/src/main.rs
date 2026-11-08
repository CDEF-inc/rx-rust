//! rx74n-hal: RI3C0 as an I2C master on P102 = SCL00-A / P103 = SDA00-A,
//! with PLL1 at 480 MHz (I3CCLK = PLL1P / 8 = 60 MHz). On the EK-RX74M
//! these pins carry the EtherCAT EEPROM (U13, 32 kbit, address 50h); the
//! test only reads it. It checks the set-up (I3CCLK, module stop, the SCL
//! timing registers), a NACK from an address nobody answers (and that the
//! bus is free again), and if the EEPROM answers (`eeprom` = 1; on the
//! EK-RX74N nothing does and those checks are skipped): a 16-byte read
//! from 0000h with a repeated START, random and current-address reads of
//! 1, 2 and 4 bytes that match it, transactions of two reads and of a read,
//! a write and a read, the same 16 bytes in Fast mode, and the
//! time of the 16-byte read in both modes (timed with CMT0 at PCLKB / 8).
//! Booted from the code MRAM by scripts/hw-run.sh; the results are in
//! `checks` / `failures` / `first_failure` / `finished`, the measurements
//! in `i2c_values`.

#![no_std]
#![no_main]
// The result variables keep the names the test scripts read.
#![allow(non_upper_case_globals)]

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

use embedded_hal::i2c::Error as _;
use embedded_hal::i2c::{ErrorKind, I2c as _, NoAcknowledgeSource, Operation};
use rx74n_hal::clock::Config;
use rx74n_hal::gpio::Parts;
use rx74n_hal::i2c::{I2c, Speed, Timing};
use rx74n_hal::pac::Peripherals;

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut first_failure: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;
#[unsafe(no_mangle)]
static mut eeprom: u32 = 0;
// The idle levels of P102 / P103 before the set-up (bit 2 / 3 of PIDR1);
// CMT0 counts of the 16-byte read at 100 / 400 kHz; the first 16 bytes
// of the EEPROM (4 words, little-endian).
#[unsafe(no_mangle)]
static mut i2c_values: [u32; 7] = [0; 7];

const EEPROM: u8 = 0x50;
const NOBODY: u8 = 0x33;

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

fn record(i: usize, v: u32) {
    unsafe { write_volatile(&raw mut i2c_values[i], v) };
}

fn rd8(a: usize) -> u8 {
    unsafe { read_volatile(a as *const u8) }
}

fn rd32(a: usize) -> u32 {
    unsafe { read_volatile(a as *const u32) }
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = Config::pll_480mhz_from_24mhz().configure(&p.sysc, &p.sram, &p.mram);
    let pins = Parts::take().unwrap();
    // P102 / P103 as inputs after reset: high with the bus pull-ups.
    let pidr1 = unsafe { read_volatile(0x87c0_0024_usize as *const u16) };
    record(0, (pidr1 >> 2 & 0b11) as u32);

    // --- the timing choice: Standard mode at 60 MHz needs CKS = 1 (30 MHz:
    // high 4.1 us = 123 - 2, low 6.0 us = 180 - 2, bus free 4.7 us = 141);
    // Fast mode CKS = 0 (high 0.7 us = 42 - 2, low 1.8 us = 108 - 2, 78)
    let std = Timing::new(60_000_000, Speed::Standard);
    let fast = Timing::new(60_000_000, Speed::Fast);
    check(1, std == Some(Timing { cks: 1, high: 121, low: 178, bus_free: 141 }));
    check(2, fast == Some(Timing { cks: 0, high: 40, low: 106, bus_free: 78 }));

    // --- CMT0 at PCLKB / 8 = 7.5 MHz for the timing
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.mstpreg.mstpcrf().modify(|_, w| w.mstpf15().clear_bit());
    p.sysc.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) });
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    p.cmt0.cmcr().write(|w| unsafe { w.cks().bits(0) });
    p.cmt0.cmcor().write(|w| unsafe { w.bits(0xffff) });
    p.cmt.cmstr0().write(|w| w.str0().set_bit());
    let cmcnt = || p.cmt0.cmcnt().read().bits();

    // --- RI3C0 in Standard mode: I3CCKCR = 05h (PLL1P), I3CCKDIVCR = 04h
    // (/8), MSTPCRB.MSTPB4 released, ICCR.ICE, ICRCCR / ICFBR / ICBFTR
    let mut i2c = I2c::ri3c0(p.ri3c0, pins.p102, pins.p103, Speed::Standard, &clocks);
    check(3, rd8(0x8701_9078) == 0x05 && rd8(0x8701_9070) == 0x04);
    check(4, rd32(0x8780_3004) & 1 << 4 == 0 && rd32(0x8745_f014) == 1 << 31);
    check(5, rd32(0x8745_f070) == 0x0001_0001 && rd32(0x8745_f074) == 0x3f3f_79b2);
    check(6, rd32(0x8745_f07c) == 141);
    // PmnPFS of P102 / P103: PSEL = 11h, PMR
    let pfs = |n: usize| rd32(0x87c0_0840 + 4 * n);
    check(7, (2..=3).all(|n| pfs(n) >> 24 & 0x7f == 0x11 && pfs(n) & 1 << 16 != 0));

    // --- nobody at 33h: an address NACK, for a write and for a read, and
    // the bus becomes free again afterwards (ICBSR.BFREE)
    let mut b = [0u8; 2];
    let w = i2c.write(NOBODY, &[0, 0]);
    let r = i2c.read(NOBODY, &mut b);
    let addr_nack = ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address);
    check(8, w.is_err_and(|e| e.kind() == addr_nack));
    check(9, r.is_err_and(|e| e.kind() == addr_nack));
    // A write of no bytes (a probe) also sees the NACK.
    check(10, i2c.write(NOBODY, &[]).is_err_and(|e| e.kind() == addr_nack));
    // BFREE comes the bus free time (4.7 us) after the STOP; allow 1 ms.
    let free = (0..100_000).any(|_| rd32(0x8745_f210) & 1 == 1);
    check(11, free);

    // --- the EEPROM at 50h, if it answers a probe
    if i2c.write(EEPROM, &[]).is_ok() {
        unsafe { write_volatile(&raw mut eeprom, 1) };
        // 16 bytes from 0000h: the address (2 bytes), a repeated START, the
        // data
        let mut all = [0u8; 16];
        let a = cmcnt();
        let ok = i2c.write_read(EEPROM, &[0x00, 0x00], &mut all).is_ok();
        let t_std = cmcnt().wrapping_sub(a) as u32;
        record(1, t_std);
        for k in 0..4 {
            record(3 + k, u32::from_le_bytes(all[4 * k..4 * k + 4].try_into().unwrap()));
        }
        check(12, ok);
        // Random reads of 1, 2, 4 bytes, then a current-address read of 4
        // (the EEPROM continues after the last byte read).
        let mut one = [0u8; 1];
        let mut two = [0u8; 2];
        let mut four = [0u8; 4];
        check(13, i2c.write_read(EEPROM, &[0x00, 0x01], &mut one).is_ok() && one[0] == all[1]);
        check(14, i2c.write_read(EEPROM, &[0x00, 0x02], &mut two).is_ok() && two == all[2..4]);
        check(15, i2c.write_read(EEPROM, &[0x00, 0x04], &mut four).is_ok() && four == all[4..8]);
        check(16, i2c.read(EEPROM, &mut four).is_ok() && four == all[8..12]);
        // A transaction of two reads in a row (no restart between them).
        let (mut x, mut y) = ([0u8; 3], [0u8; 5]);
        let ok = i2c
            .transaction(
                EEPROM,
                &mut [
                    Operation::Write(&[0x00, 0x03]),
                    Operation::Read(&mut x),
                    Operation::Read(&mut y),
                ],
            )
            .is_ok();
        check(17, ok && x == all[3..6] && y == all[6..11]);
        // A read, then a write: the read's last byte already requests the
        // repeated START for the write. The write only sets the address
        // (no data bytes, so nothing is programmed).
        let ok = i2c
            .transaction(
                EEPROM,
                &mut [
                    Operation::Read(&mut x),
                    Operation::Write(&[0x00, 0x05]),
                    Operation::Read(&mut y),
                ],
            )
            .is_ok();
        check(22, ok && y == all[5..10]);
        // 16 bytes: (2 + 1) x 9 + 16 x 9 = 171 bits at most 100 kHz: at
        // least 1.71 ms (12825 counts), at most 2.5 ms.
        check(18, (12_825..18_750).contains(&t_std));

        // --- Fast mode: the same bytes, at least 4 times faster
        let (ri3c, scl, sda) = i2c.free();
        i2c = I2c::ri3c0(ri3c, scl, sda, Speed::Fast, &clocks);
        check(19, rd32(0x8745_f070) == 0x0001_0000 && rd32(0x8745_f074) == 0x3f3f_286a);
        let mut again = [0u8; 16];
        let a = cmcnt();
        let ok = i2c.write_read(EEPROM, &[0x00, 0x00], &mut again).is_ok();
        let t_fast = cmcnt().wrapping_sub(a) as u32;
        record(2, t_fast);
        check(20, ok && again == all);
        // 171 bits at most 400 kHz: at least 0.43 ms (3207 counts)
        check(21, t_fast >= 3_207 && t_fast * 3 < t_std);
    }

    let _ = i2c.free();
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
