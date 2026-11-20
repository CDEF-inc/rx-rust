//! The HAL's interrupt-driven I2C (embedded-hal-async `I2c`) on the
//! board's EEPROM at 50h, which the test only reads:
//!
//! - AP-RX72M-0A: SCI6's simple I2C (`AsyncSciI2c`) on the general-purpose
//!   EEPROM (24LC16B, a one-byte word address), 100 kHz;
//! - EK-RX74M: RI3C0 (`AsyncI2c`) on the EtherCAT EEPROM U13 (a two-byte
//!   address), Standard and Fast mode. The EK-RX74N has no EEPROM there:
//!   the EEPROM checks are skipped (`eeprom` = 0).
//!
//! Checks:
//!
//! - a NACK from an address nobody answers (write and read), and transfers
//!   after it;
//! - the first 16 bytes by the blocking driver, then by `write_read`,
//!   random reads of 1 / 4 bytes, a transaction of a write and two reads,
//!   and `read` (current address) after a `write` of the address;
//! - a transaction cut short (polled once, dropped), and a transfer after
//!   it;
//! - (RX74) Fast mode; `free` back to the blocking driver.
//!
//! Booted from the code flash / MRAM by scripts/hw-run.sh (scripts/hw-build.sh
//! builds it); the results are left in hw-board's `checks` / `failures` /
//! `first_failure` / `finished`.

#![no_std]
#![no_main]

use core::future::Future;
use core::mem::MaybeUninit;
use core::pin::pin;
use core::ptr::write_volatile;
use core::task::{Context, Waker};

use embedded_hal::i2c::{ErrorKind, I2c as _, NoAcknowledgeSource, Operation};
use embedded_hal_async::i2c::I2c as _;
use hw_board::check;
use hw_board::hal::{self, clock::Clocks, gpio::Parts, icu::Priority, pac::Peripherals};
use rx_rt::executor::{Executor, Spawner};

#[cfg(feature = "ap-rx72m")]
mod board {
    pub use hw_board::hal::sci_i2c::SciI2c as Blocking;
    hw_board::hal::bind_sci_i2c_interrupts!();
    hw_board::hal::bind_group_bl0!();
    /// The word address: one byte (the 24LC16B's block bits stay 0).
    pub fn at(off: u8) -> ([u8; 2], usize) {
        ([off, 0], 1)
    }
}

#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
mod board {
    pub use hw_board::hal::i2c::I2c as Blocking;
    hw_board::hal::bind_i2c_interrupts!();
    hw_board::hal::bind_group_al1!();
    /// The word address: two bytes, high first.
    pub fn at(off: u8) -> ([u8; 2], usize) {
        ([0, off], 2)
    }
}

use board::{Blocking, at};

#[unsafe(no_mangle)]
static mut eeprom: u32 = 0;

const EEPROM: u8 = 0x50;
const NOBODY: u8 = 0x33;

struct Board {
    i2c: Blocking,
    #[allow(dead_code)]
    clocks: Clocks,
}

#[cfg(feature = "ap-rx72m")]
fn setup(p: Peripherals) -> Board {
    let clocks = hal::clock::Config::pll_240mhz_from_24mhz().configure(&p.system);
    let pins = Parts::take().unwrap();
    let i2c = Blocking::sci6(p.sci6, pins.p00, pins.p01, &clocks);
    Board { i2c, clocks }
}

#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
fn setup(p: Peripherals) -> Board {
    let clocks = hal::clock::Config::pll_480mhz_from_24mhz().configure(&p.sysc, &p.sram, &p.mram);
    let pins = Parts::take().unwrap();
    let i2c = Blocking::ri3c0(p.ri3c0, pins.p102, pins.p103, hal::i2c::Speed::Standard, &clocks);
    Board { i2c, clocks }
}

fn nack_address<E: embedded_hal::i2c::Error>(r: Result<(), E>) -> bool {
    matches!(r, Err(e) if e.kind() == ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address))
}

#[embassy_executor::task]
async fn run(_spawner: Spawner, b: Board) {
    let mut blocking = b.i2c;
    let (a0, n0) = at(0);
    let mut want = [0u8; 16];
    let present = blocking.write_read(EEPROM, &a0[..n0], &mut want).is_ok();
    unsafe { write_volatile(&raw mut eeprom, present as u32) };

    let mut i2c = blocking.into_async(Priority::new(3).unwrap());

    // A NACK from nobody, then the bus is usable again.
    check(1, nack_address(i2c.write(NOBODY, &[0]).await));
    check(2, nack_address(i2c.read(NOBODY, &mut [0u8; 2]).await));

    if present {
        let mut got = [0u8; 16];
        check(3, i2c.write_read(EEPROM, &a0[..n0], &mut got).await.is_ok() && got == want);
        // Random reads.
        let (a, n) = at(5);
        let mut one = [0u8; 1];
        check(4, i2c.write_read(EEPROM, &a[..n], &mut one).await.is_ok() && one[0] == want[5]);
        let (a, n) = at(9);
        let mut four = [0u8; 4];
        check(5, i2c.write_read(EEPROM, &a[..n], &mut four).await.is_ok() && four == want[9..13]);
        // A write and two reads in one transaction.
        let (a, n) = at(2);
        let (mut x, mut y) = ([0u8; 3], [0u8; 5]);
        let r = i2c
            .transaction(
                EEPROM,
                &mut [Operation::Write(&a[..n]), Operation::Read(&mut x), Operation::Read(&mut y)],
            )
            .await;
        check(6, r.is_ok() && x == want[2..5] && y == want[5..10]);
        // The current address after a write of the address alone.
        let (a, n) = at(7);
        let mut cur = [0u8; 4];
        let r1 = i2c.write(EEPROM, &a[..n]).await;
        let r2 = i2c.read(EEPROM, &mut cur).await;
        check(7, r1.is_ok() && r2.is_ok() && cur == want[7..11]);

        // A transaction cut short: polled once (the start condition and the
        // address on their way), then dropped; the next one still works.
        {
            let mut long = [0u8; 16];
            let mut fut = pin!(i2c.write_read(EEPROM, &a0[..n0], &mut long));
            let mut cx = Context::from_waker(Waker::noop());
            check(8, fut.as_mut().poll(&mut cx).is_pending());
        }
        let (a, n) = at(0);
        let mut got = [0u8; 16];
        check(9, i2c.write_read(EEPROM, &a[..n], &mut got).await.is_ok() && got == want);

        // RX74: Fast mode, through the blocking driver's set-up.
        #[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
        let mut i2c = {
            let (ri3c, scl, sda) = i2c.free().free();
            let blocking = Blocking::ri3c0(ri3c, scl, sda, hal::i2c::Speed::Fast, &b.clocks);
            let mut i2c = blocking.into_async(Priority::new(3).unwrap());
            let mut got = [0u8; 16];
            check(10, i2c.write_read(EEPROM, &a[..n], &mut got).await.is_ok() && got == want);
            i2c
        };
        let _ = &mut i2c;
        let mut blocking = i2c.free();
        let mut got = [0u8; 16];
        check(11, blocking.write_read(EEPROM, &a[..n], &mut got).is_ok() && got == want);
    }
    hw_board::finish()
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let b = setup(Peripherals::take().unwrap());
    static mut EXECUTOR: MaybeUninit<Executor> = MaybeUninit::uninit();
    let executor = unsafe { (&raw mut EXECUTOR).as_mut().unwrap().write(Executor::new()) };
    executor.run(|spawner| spawner.spawn(run(spawner, b).unwrap()))
}
