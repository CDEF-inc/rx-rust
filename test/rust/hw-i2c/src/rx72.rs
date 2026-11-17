//! rx72-hal on the AP-RX72M-0A board, PLL 240 MHz (PCLKB 60 MHz): RIIC0 on CN8
//! (P12 = SCL0, P13 = SDA0) and SCI6's simple I2C on the board's EEPROM
//! (P00 = SSDA6, P01 = SSCL6). See docs/NOTES-hal.md.
//!
//! RIIC0: the timing choice and the set-up always; the bus checks depend on
//! what is on CN8 (`i2c_bus`), found from SDA alone so that SCL, which the
//! USB serial adapter's TXD may drive, is never driven by the test:
//! - 0: anything else (the adapter: its weak RXD pull-up is slow): skipped.
//! - 1: nothing: with the internal pull-ups, the address NACK from 33h for a
//!   write, a read and a probe, and the bus free afterwards.
//! - 2: a bus with pull-ups: those, a scan of 08h-77h, and if a 1-byte-address
//!   EEPROM answers at 50h (`eeprom` bit 0), the read-only EEPROM checks of
//!   the EK-RX74M's hw-i2c in Standard and Fast mode.
//!
//! SCI6, always: the set-up, 16 bytes from 00h (the MAC addresses in the
//! first 12, so not blank), random reads that match, the address NACK from
//! 33h and a current-address read after it (`eeprom` bit 1).

use core::ptr::{read_volatile, write_volatile};

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::InputPin;
use embedded_hal::i2c::{ErrorKind, I2c as _, NoAcknowledgeSource, Operation};
use hw_board::hal::clock::Config;
use hw_board::hal::delay::Delay;
use hw_board::hal::gpio::{Input, IoCapable, Parts, Pin};
use hw_board::hal::i2c::{I2c, Speed, Timing};
use hw_board::hal::pac::Peripherals;
use hw_board::hal::sci_i2c::{Rate, SciI2c};
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
// [16..20] the first 16 bytes of the EEPROM through SCI6; [20] / [21] the
// results of that read and of the probe of 33h through SCI6.
#[unsafe(no_mangle)]
static mut i2c_values: [u32; 22] = [0; 22];

const EEPROM: u8 = 0x50;
const NOBODY: u8 = 0x33;
/// The most polls for a pin to rise through I2C pull-ups.
const FAST_RISE: u32 = 2;

fn record(i: usize, v: u32) {
    unsafe { write_volatile(&raw mut i2c_values[i], v) };
}

/// 0 Ok, 1 / 2 / 3 NACK on the address / data / unknown, 4 arbitration
/// lost, 5 other (the timeout), 6 anything else.
fn code<T, E: embedded_hal::i2c::Error>(r: &Result<T, E>) -> u32 {
    match r.as_ref().map_err(|e| e.kind()) {
        Ok(_) => 0,
        Err(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address)) => 1,
        Err(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Data)) => 2,
        Err(ErrorKind::NoAcknowledge(_)) => 3,
        Err(ErrorKind::ArbitrationLoss) => 4,
        Err(ErrorKind::Other) => 5,
        Err(_) => 6,
    }
}

fn rd8(a: usize) -> u8 {
    unsafe { read_volatile(a as *const u8) }
}

/// The polls of PIDR until the pin reads high again after being driven low
/// for 2 us and released without the pull-up (at most FFFFh).
fn rise<const P: usize, const N: u8, M>(
    pin: Pin<P, N, M>,
    delay: &mut Delay,
) -> (u32, Pin<P, N, Input>)
where
    (): IoCapable<P, N>,
{
    let out = pin.into_push_pull_output();
    delay.delay_us(2);
    let mut pin = out.into_input();
    let mut n = 0;
    while n < 0xffff && pin.is_low().unwrap() {
        n += 1;
    }
    (n, pin)
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = Config::pll_240mhz_from_24mhz().configure(&p.system);
    let pins = Parts::take().unwrap();

    let mut delay = Delay::new(p.cmt2, &clocks);

    // --- what is on CN8: the rise of SDA (P13; the adapter's RXD, an input
    // on its side), and the level of SCL (P12), only read ([0] = polls | SCL
    // high << 16). I2C pull-ups bring SDA back at once (the board's EEPROM
    // bus: 0 polls); the adapter's RXD pull-up took 4-9.
    let mut scl_in = pins.p12.into_input();
    let (r13, sda_in) = rise(pins.p13, &mut delay);
    let scl_high = scl_in.is_high().unwrap();
    record(0, r13 | (scl_high as u32) << 16);
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
    let mut sci = SciI2c::sci6(p.sci6, pins.p00, pins.p01, &clocks);
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
    record(20, code(&r));
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
    record(21, code(&r));
    check(31, code(&r) == 1);
    check(32, sci.read(EEPROM, &mut four).is_ok() && four == all[10..14]);
    let _ = sci.free();
    p.cmt.cmstr0().write(|w| w.str0().clear_bit());
    hw_board::finish()
}
