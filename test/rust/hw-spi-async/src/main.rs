//! The HAL's DMA-driven SPI (`spi::AsyncSpi`, embedded-hal-async
//! `SpiBus`) on the board, through the RSPI's internal loopback (no
//! wiring; RSPCK and MOSI still go out on their pins), at 15 MHz:
//!
//! - `transfer` of equal lengths, `transfer_in_place`, `read` (FFh sent),
//!   `write`, and `transfer` with a longer `read` and a longer `write`;
//! - the inverted loopback (SPLP) after `free`, the blocking driver again,
//!   and `into_async` once more;
//! - 70000 bytes in place: more than one DMA transfer (65535 units);
//! - none of the RSPI's receive / transmit requests reaches the CPU.
//!
//! Booted from the code flash / MRAM by scripts/hw-run.sh (scripts/hw-build.sh
//! builds it); the results are left in hw-board's `checks` / `failures` /
//! `first_failure` / `finished`.

#![no_std]
#![no_main]

use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use embedded_hal::spi::{MODE_0, SpiBus as _};
use embedded_hal_async::spi::SpiBus;
use hw_board::check;
use hw_board::hal::{
    self,
    dma::{self, Dma},
    gpio::Parts,
    icu::Priority,
    pac::Peripherals,
    spi::Spi,
};
use rx_rt::executor::{Executor, Spawner};

hal::bind_dma_interrupts!();

/// The RSPI's receive / transmit vectors on this board.
#[cfg(feature = "ap-rx72m")]
mod vectors {
    rx_rt::interrupt!(108, super::on_rspi);
    rx_rt::interrupt!(109, super::on_rspi);
}

#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
mod vectors {
    rx_rt::interrupt!(146, super::on_rspi);
    rx_rt::interrupt!(147, super::on_rspi);
}

/// RSPI requests that reached the CPU.
static CPU_RSPI: AtomicU32 = AtomicU32::new(0);

extern "C" fn on_rspi() {
    CPU_RSPI.fetch_add(1, Relaxed);
}

#[cfg(feature = "ap-rx72m")]
fn setup(p: Peripherals) -> (Spi, Dma) {
    let clocks = hal::clock::Config::pll_240mhz_from_24mhz().configure(&p.system);
    let pins = Parts::take().unwrap();
    let spi = Spi::rspi2(p.rspi2, pins.p56, pins.p54, pins.p55, MODE_0, 15_000_000, &clocks);
    (spi, Dma::new(p.dmac))
}

#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
fn setup(p: Peripherals) -> (Spi, Dma) {
    let clocks = hal::clock::Config::pll_480mhz_from_24mhz().configure(&p.sysc, &p.sram, &p.mram);
    let pins = Parts::take().unwrap();
    let spi = Spi::rspia0(p.rspia0, pins.pa05, pins.pa06, pins.pa07, MODE_0, 15_000_000, &clocks);
    (spi, Dma::new(p.dmac))
}

const N: usize = 70_000;
static mut BIG: [u8; N] = [0; N];

#[embassy_executor::task]
async fn run(_spawner: Spawner, spi: Spi, dma: Dma) {
    dma::enable_interrupts(Priority::new(3).unwrap());
    let mut blocking = spi;
    blocking.set_loopback(Some(false)).unwrap();
    let mut spi = blocking.into_async(dma.ch0, dma.ch1);

    let tx: [u8; 64] = core::array::from_fn(|i| (i as u8).wrapping_mul(37) ^ 0x5a);
    let mut rx = [0u8; 64];
    check(1, spi.transfer(&mut rx, &tx).await.is_ok() && rx == tx);

    let mut buf: [u8; 300] = core::array::from_fn(|i| i as u8);
    let want = buf;
    check(2, spi.transfer_in_place(&mut buf).await.is_ok() && buf == want);

    let mut r = [0u8; 16];
    check(3, spi.read(&mut r).await.is_ok() && r.iter().all(|&b| b == 0xff));
    check(4, spi.write(&tx).await.is_ok());

    // A longer read: FFh comes back past the end of write.
    let mut r = [0u8; 40];
    check(5, spi.transfer(&mut r, &tx[..10]).await.is_ok());
    check(6, r[..10] == tx[..10] && r[10..].iter().all(|&b| b == 0xff));
    // A longer write: the bytes past read are dropped.
    let mut r = [0u8; 8];
    check(7, spi.transfer(&mut r, &tx).await.is_ok() && r == tx[..8]);
    check(8, spi.flush().await.is_ok());

    // Back to the blocking driver, the inverted loopback, async again.
    let (mut blocking, ch0, ch1) = spi.free();
    let mut b = [0x0fu8, 0xa5];
    check(9, blocking.set_loopback(Some(true)).is_ok());
    check(10, blocking.transfer_in_place(&mut b).is_ok() && b == [0xf0, 0x5a]);
    let mut spi = blocking.into_async(ch0, ch1);
    let mut r = [0u8; 64];
    check(11, spi.transfer(&mut r, &tx).await.is_ok());
    check(12, r.iter().zip(tx.iter()).all(|(&a, &b)| a == !b));
    let (mut blocking, ch0, ch1) = spi.free();
    check(13, blocking.set_loopback(Some(false)).is_ok());
    let mut spi = blocking.into_async(ch0, ch1);

    // Over one DMA transfer's 65535 units.
    let big = unsafe { &mut *(&raw mut BIG) };
    for (i, x) in big.iter_mut().enumerate() {
        *x = (i ^ (i >> 8)) as u8;
    }
    check(14, spi.transfer_in_place(big).await.is_ok());
    check(15, big.iter().enumerate().all(|(i, &x)| x == (i ^ (i >> 8)) as u8));

    check(16, CPU_RSPI.load(Relaxed) == 0);
    hw_board::finish()
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let (spi, dma) = setup(Peripherals::take().unwrap());
    static mut EXECUTOR: MaybeUninit<Executor> = MaybeUninit::uninit();
    let executor = unsafe { (&raw mut EXECUTOR).as_mut().unwrap().write(Executor::new()) };
    executor.run(|spawner| spawner.spawn(run(spawner, spi, dma).unwrap()))
}
