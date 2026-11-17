//! rx72-hal on the AP-RX72M-0A board: RIIC0 as an I2C master on CN8
//! (pin 1 = P12 = SCL0, pin 2 = P13 = SDA0, pin 5 = VDD, pin 6 = GND), with
//! the PLL at 240 MHz (PCLKB 60 MHz). It checks the timing choice and the
//! set-up (module stop, the RIIC registers, the pin functions). What is on
//! CN8 decides the rest (`i2c_bus`):
//!
//! - 0: anything else, such as the USB serial adapter of serial-echo (its
//!   TXD drives P12 high, and SDA has only its weak RXD pull-up). The bus
//!   checks are skipped, so that the RIIC does not pull its TXD low.
//! - 1: nothing: the internal pull-ups carry the bus. A NACK from an
//!   address nobody answers, for a write, a read and a write of no bytes,
//!   and the bus free afterwards.
//! - 2: a bus with pull-ups: the NACK checks, a scan of addresses 08h-77h
//!   (the ACKs in `i2c_values`), and if an EEPROM with one address byte
//!   answers at 50h (`eeprom` bit 0; a 24C01-24C16, such as the board's own
//!   24LC16B on P00 / P01 when CN2 pin 26 is wired to CN8 pin 1 and CN2
//!   pin 25 to CN8 pin 2): the reads of hw-i2c on the EK-RX74M, read only.
//!   A 16-byte read from 00h with a repeated START, random and
//!   current-address reads of 1, 2 and 4 bytes that match it, transactions
//!   of two reads and of a read, a write and a read, the same 16 bytes in
//!   Fast mode, and the time of the 16-byte read in both modes (timed with
//!   CMT0 at PCLKB / 8).
//!
//! The kind of bus is found from SDA (P13) alone, as GPIO: it is driven
//! low for a moment, then left as an input without the pull-up, and PIDR is
//! polled until it reads high. SCL (P12) is only read, never driven: the
//! adapter's TXD drives it, and driving it low would fight that output.
//! P13 is our TXD2 and the adapter's RXD, an input on its side, so it may
//! be driven. An I2C pull-up brings SDA back within [`FAST_RISE`] polls
//! (the board's EEPROM bus, measured as a reference, takes none) and SCL
//! reads high; the adapter's RXD pull-up takes 6-9 polls; a floating pin
//! stays low. An adapter with an RXD pull-up as strong as an I2C one would
//! be taken for a bus, and the RIIC would then pull its TXD low until the
//! timeout.
//!
//! Then, on every run, SCI6 in simple I2C mode reads the board's own
//! EEPROM on P00 / P01 (24LC16B at 50h, JSW6 ON): the register set-up, 16
//! bytes from 00h (the board's MAC addresses in the first 12, so not
//! blank), random reads that match them, an address NACK from 33h, and a
//! current-address read after it (`eeprom` bit 1 when the read works).
//! Booted from the code flash by scripts/hw-run.sh; the results are in
//! `checks` / `failures` / `first_failure` / `finished`, the measurements
//! in `i2c_values`.

use core::ptr::{read_volatile, write_volatile};

use embedded_hal::digital::InputPin;
use embedded_hal::i2c::{I2c as _, NoAcknowledgeSource, Operation};
use hw_board::hal::clock::Config;
use hw_board::hal::gpio::Parts;
use hw_board::hal::i2c::{Error, I2c, Speed, Timing};
use hw_board::hal::pac::Peripherals;
use hw_board::hal::sci_i2c::{Error as SciError, Rate, SciI2c};
use hw_board::check;

#[unsafe(no_mangle)]
static mut eeprom: u32 = 0;
#[unsafe(no_mangle)]
static mut i2c_bus: u32 = 0;
// [0] the polls until P13 rises, and bit 16 P12 high; [1] ICCR1
// after the set-up; [2] / [3] CMT0 counts of the 16-byte read at 100 /
// 400 kHz; [4..8] the first 16 bytes of the EEPROM (little-endian words);
// [8..12] the addresses that answered the scan (bit n of word n / 32);
// [12..15] the results of the write, read and probe to 33h (`code`);
// [15] ICCR2 / ICSR2 after them (bits 15:8 / 7:0); [16..20] the first 16
// bytes of the EEPROM through SCI6; [20] / [21] the results of that read
// and of the probe of 33h through SCI6 (`code_sci`); [22] the
// polls until P01 / P00 rise.
#[unsafe(no_mangle)]
static mut i2c_values: [u32; 23] = [0; 23];

const EEPROM: u8 = 0x50;
const NOBODY: u8 = 0x33;
/// The most polls for a pin to rise through I2C pull-ups.
const FAST_RISE: u32 = 2;

fn record(i: usize, v: u32) {
    unsafe { write_volatile(&raw mut i2c_values[i], v) };
}

/// 0 Ok, 1 / 2 / 3 NACK on the address / data / unknown, 4 arbitration
/// lost, 5 timeout.
fn code<T>(r: &Result<T, Error>) -> u32 {
    match r {
        Ok(_) => 0,
        Err(Error::NoAcknowledge(NoAcknowledgeSource::Address)) => 1,
        Err(Error::NoAcknowledge(NoAcknowledgeSource::Data)) => 2,
        Err(Error::NoAcknowledge(_)) => 3,
        Err(Error::ArbitrationLoss) => 4,
        Err(Error::Timeout) => 5,
    }
}

fn code_sci<T>(r: &Result<T, SciError>) -> u32 {
    match r {
        Ok(_) => 0,
        Err(SciError::NoAcknowledge(NoAcknowledgeSource::Address)) => 1,
        Err(SciError::NoAcknowledge(NoAcknowledgeSource::Data)) => 2,
        Err(SciError::NoAcknowledge(_)) => 3,
        Err(SciError::Timeout) => 5,
    }
}

fn rd8(a: usize) -> u8 {
    unsafe { read_volatile(a as *const u8) }
}

/// Over 2 us at 240 MHz (a volatile read takes several cycles; core::hint::
/// spin_loop is nothing on RX).
fn pause() {
    let keep = 0u32;
    for _ in 0..500 {
        unsafe { read_volatile(&keep) };
    }
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = Config::pll_240mhz_from_24mhz().configure(&p.system);
    let pins = Parts::take().unwrap();

    // --- what is on CN8: the polls of PIDR until SDA (P13) reads high again
    // after a discharge, without the pull-up, and the level of SCL (P12),
    // which is only read ([0] = polls | SCL high << 16). The board's EEPROM
    // bus (P01 = SSCL6, P00 = SSDA6) is measured the same way as a reference
    // ([22] = P01 | P00 << 16): its pull-ups bring both back within the
    // first poll.
    macro_rules! rise {
        ($pin:expr) => {{
            let o = $pin.into_push_pull_output();
            pause();
            let mut i = o.into_input();
            let mut n = 0u32;
            while n < 0xffff && i.is_low().unwrap() {
                n += 1;
            }
            (n, i)
        }};
    }
    let mut scl_in = pins.p12.into_input();
    let (r13, sda_in) = rise!(pins.p13);
    let scl_high = scl_in.is_high().unwrap();
    let (r01, p01) = rise!(pins.p01);
    let (r00, p00) = rise!(pins.p00);
    record(0, r13 | (scl_high as u32) << 16);
    record(22, r01 | r00 << 16);
    // SDA back at once and SCL high: pull-ups (2); SDA still low after the
    // polls of over 1 ms: floating (1); anything else (a slow pull-up, or
    // SCL low): not a bus to test (0).
    let bus = if r13 <= FAST_RISE && scl_high {
        2
    } else if r13 == 0xffff {
        1
    } else {
        0
    };
    unsafe { write_volatile(&raw mut i2c_bus, bus) };
    // The internal pull-ups stay on in the RIIC function.
    let (scl, sda) = (scl_in.into_pull_up_input(), sda_in.into_pull_up_input());

    // --- the timing choice at PCLKB 60 MHz: Standard mode CKS = 4 (3.75
    // MHz: high 4.1 us = 16 cycles, low 6.0 us = 23); Fast mode CKS = 2
    // (15 MHz: high 0.7 us = 11, low 1.8 us = 27)
    let std = Timing::new(60_000_000, Speed::Standard);
    let fast = Timing::new(60_000_000, Speed::Fast);
    check(1, std == Some(Timing { cks: 4, brh: 15, brl: 22 }));
    check(2, fast == Some(Timing { cks: 2, brh: 10, brl: 26 }));
    check(3, std.unwrap().hz(60_000_000) == 96_153 && fast.unwrap().hz(60_000_000) == 394_736);

    // --- CMT0 at PCLKB / 8 = 7.5 MHz for the timing
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.system.mstpcra().modify(|_, w| w.mstpa15().clear_bit());
    p.system.prcr().write(|w| unsafe { w.prkey().bits(0xa5) });
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    p.cmt0.cmcr().write(|w| unsafe { w.cks().bits(0) });
    p.cmt0.cmcor().write(|w| unsafe { w.bits(0xffff) });
    p.cmt.cmstr0().write(|w| w.str0().set_bit());
    let cmcnt = || p.cmt0.cmcnt().read().bits();

    // --- RIIC0 in Standard mode: MSTPCRB.MSTPB21 released; ICCR1 = ICE,
    // SOWP, SCLO / SDAO released; ICMR1 = CKS 4, BCWP; ICBRH / ICBRL;
    // ICFER = SCLE, NFE, NACKE, MALE, TMOE; no slave address; ICMR3.ACKWP
    let mut i2c = I2c::riic0(p.riic0, scl, sda, Speed::Standard, &clocks);
    let iccr1 = rd8(0x0008_8300);
    record(1, iccr1 as u32);
    check(4, p.system.mstpcrb().read().mstpb21().bit_is_clear() && iccr1 & 0xfc == 0x9c);
    check(5, rd8(0x0008_8302) == 0x48 && rd8(0x0008_8311) == 0xef && rd8(0x0008_8310) == 0xf6);
    check(6, rd8(0x0008_8305) == 0x73 && rd8(0x0008_8306) == 0 && rd8(0x0008_8304) == 0x10);
    // P12PFS / P13PFS: PSEL = 0Fh; PMR of P12 / P13; PCR kept
    check(7, rd8(0x0008_c14a) == 0x0f && rd8(0x0008_c14b) == 0x0f);
    check(8, rd8(0x0008_c061) & 0b1100 == 0b1100 && rd8(0x0008_c0c1) & 0b1100 == 0b1100);

    if bus != 0 {
        // --- the bus idles high: ICCR1.SCLI / SDAI
        check(9, rd8(0x0008_8300) & 0b11 == 0b11);
        // --- nobody at 33h: an address NACK, for a write and for a read,
        // and the bus becomes free again afterwards (ICCR2.BBSY)
        let mut b = [0u8; 2];
        let w = i2c.write(NOBODY, &[0, 0]);
        let r = i2c.read(NOBODY, &mut b);
        // A write of no bytes (a probe) also sees the NACK.
        let pr = i2c.write(NOBODY, &[]);
        record(12, code(&w));
        record(13, code(&r));
        record(14, code(&pr));
        record(15, (rd8(0x0008_8301) as u32) << 8 | rd8(0x0008_8309) as u32);
        check(10, code(&w) == 1);
        check(11, code(&r) == 1);
        check(12, code(&pr) == 1);
        // BBSY clears the bus free time (6.1 us) after the STOP; allow 1 ms.
        let free = (0..100_000).any(|_| rd8(0x0008_8301) & 0x80 == 0);
        check(13, free);
    }

    if bus == 2 {
        // --- the scan: a probe of each address
        let mut found = [0u32; 4];
        for a in 0x08..0x78u8 {
            if i2c.write(a, &[]).is_ok() {
                found[a as usize / 32] |= 1 << (a % 32);
            }
        }
        for (k, &f) in found.iter().enumerate() {
            record(8 + k, f);
        }
        check(14, found[NOBODY as usize / 32] & 1 << (NOBODY % 32) == 0);
    }

    // --- the EEPROM at 50h, if it answers a probe
    if bus == 2 && i2c.write(EEPROM, &[]).is_ok() {
        unsafe { write_volatile(&raw mut eeprom, 1) };
        // 16 bytes from 00h: the address (1 byte), a repeated START, the data
        let mut all = [0u8; 16];
        let a = cmcnt();
        let ok = i2c.write_read(EEPROM, &[0x00], &mut all).is_ok();
        let t_std = cmcnt().wrapping_sub(a) as u32;
        record(2, t_std);
        for k in 0..4 {
            record(4 + k, u32::from_le_bytes(all[4 * k..4 * k + 4].try_into().unwrap()));
        }
        check(15, ok);
        // Random reads of 1, 2, 4 bytes, then a current-address read of 4
        // (the EEPROM continues after the last byte read).
        let mut one = [0u8; 1];
        let mut two = [0u8; 2];
        let mut four = [0u8; 4];
        check(16, i2c.write_read(EEPROM, &[0x01], &mut one).is_ok() && one[0] == all[1]);
        check(17, i2c.write_read(EEPROM, &[0x02], &mut two).is_ok() && two == all[2..4]);
        check(18, i2c.write_read(EEPROM, &[0x04], &mut four).is_ok() && four == all[4..8]);
        check(19, i2c.read(EEPROM, &mut four).is_ok() && four == all[8..12]);
        // A transaction of two reads in a row (no restart between them).
        let (mut x, mut y) = ([0u8; 3], [0u8; 5]);
        let ok = i2c
            .transaction(
                EEPROM,
                &mut [Operation::Write(&[0x03]), Operation::Read(&mut x), Operation::Read(&mut y)],
            )
            .is_ok();
        check(20, ok && x == all[3..6] && y == all[6..11]);
        // A read, then a write: the read's last byte already requests the
        // repeated START for the write. The write only sets the address
        // (no data bytes, so nothing is programmed).
        let ok = i2c
            .transaction(
                EEPROM,
                &mut [Operation::Read(&mut x), Operation::Write(&[0x05]), Operation::Read(&mut y)],
            )
            .is_ok();
        check(21, ok && y == all[5..10]);
        // 16 bytes: (1 + 1 + 1) x 9 + 16 x 9 = 171 bits at most 100 kHz: at
        // least 1.71 ms (12825 counts), at most 3 ms.
        check(22, (12_825..22_500).contains(&t_std));

        // --- Fast mode: the same bytes, at least 3 times faster
        let (riic, scl, sda) = i2c.free();
        i2c = I2c::riic0(riic, scl, sda, Speed::Fast, &clocks);
        check(23, rd8(0x0008_8302) == 0x28 && rd8(0x0008_8311) == 0xea && rd8(0x0008_8310) == 0xfa);
        let mut again = [0u8; 16];
        let a = cmcnt();
        let ok = i2c.write_read(EEPROM, &[0x00], &mut again).is_ok();
        let t_fast = cmcnt().wrapping_sub(a) as u32;
        record(3, t_fast);
        check(24, ok && again == all);
        // 171 bits at most 400 kHz: at least 0.43 ms (3207 counts)
        check(25, t_fast >= 3_207 && t_fast * 3 < t_std);
    }

    let _ = i2c.free();

    // --- SCI6 in simple I2C mode on P00 = SSDA6 / P01 = SSCL6: the board's
    // EEPROM (24LC16B at 50h, JSW6 ON). SMR.CKS = 0, BRR = 18: 60 MHz /
    // (32 x 19) = 98.7 kHz.
    let rate = Rate::new(60_000_000, 100_000);
    check(26, rate == Some(Rate { cks: 0, brr: 18 }) && rate.unwrap().hz(60_000_000) == 98_684);
    let mut sci = SciI2c::sci6(p.sci6, p00, p01, &clocks);
    // SMR, BRR, SCMR (SDIR), SIMR1 (IICDL 18h, IICM), SIMR2 (IICACKT,
    // IICCSC, IICINTM), SIMR3 released; PSEL 0Ah; ODR0 B0 / B2
    check(
        27,
        rd8(0x0008_a0c0) == 0
            && rd8(0x0008_a0c1) == 18
            && rd8(0x0008_a0c6) == 0xfa
            && rd8(0x0008_a0c9) == 0xc1
            && rd8(0x0008_a0ca) == 0x23
            && rd8(0x0008_a0cb) == 0xf0
            && rd8(0x0008_c140) == 0x0a
            && rd8(0x0008_c141) == 0x0a
            && rd8(0x0008_c080) & 0b101 == 0b101,
    );
    let mut all = [0u8; 16];
    let r = sci.write_read(EEPROM, &[0x00], &mut all);
    record(20, code_sci(&r));
    for k in 0..4 {
        record(16 + k, u32::from_le_bytes(all[4 * k..4 * k + 4].try_into().unwrap()));
    }
    check(28, r.is_ok());
    if r.is_ok() {
        unsafe { write_volatile(&raw mut eeprom, read_volatile(&raw const eeprom) | 2) };
    }
    // The first 12 bytes hold the board's MAC addresses, so they are not
    // blank (FFh).
    check(29, all[..12].iter().any(|&b| b != 0xff));
    let mut one = [0u8; 1];
    let mut four = [0u8; 4];
    let ok = sci.write_read(EEPROM, &[0x01], &mut one).is_ok() && one[0] == all[1];
    check(30, ok && sci.write_read(EEPROM, &[0x06], &mut four).is_ok() && four == all[6..10]);
    // Nobody at 33h: an address NACK; then the EEPROM still answers, and
    // a current-address read continues after the last byte read (0Ah).
    let r = sci.write(NOBODY, &[]);
    record(21, code_sci(&r));
    check(31, r == Err(SciError::NoAcknowledge(NoAcknowledgeSource::Address)));
    check(32, sci.read(EEPROM, &mut four).is_ok() && four == all[10..14]);
    let _ = sci.free();
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    hw_board::finish()
}
