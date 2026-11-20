//! serial-echo with async tasks: echoes what arrives on the board's serial
//! port (115200 bps 8N1) through the HAL's interrupt-driven `AsyncSerial`
//! (`embedded-io-async`), while a second task counts 100 ms ticks on
//! embassy-time (the HAL's time driver on CMTW0 / CMTW1). Both run on
//! rx-rt's executor, which sleeps in `wait` while neither has work.
//!
//! The board is a feature: `ek-rx74n` / `ek-rx74m`, the USB serial port J27
//! (RSCI12, P114 / P115) with CPUCLK0 at 600 MHz. (The AP-RX72M-0A's SCI2 has
//! no `AsyncSerial` yet.)
//!
//! scripts/hw/serial-echo.ps1 checks it from Windows, as it does serial-echo.
//! `RX_BYTES` / `RX_ERRORS` count what arrived and `TICKS` the ticks.

#![no_std]
#![no_main]

use core::mem::MaybeUninit;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_time::Timer;
use embedded_io_async::{Read, Write};
use rx_rt::executor::{Executor, Spawner};
use rx74_hal::clock::Config;
use rx74_hal::gpio::Parts;
use rx74_hal::icu::Priority;
use rx74_hal::pac::Peripherals;
use rx74_hal::serial::{AsyncSerial, Error, Serial};

#[cfg(not(any(feature = "ek-rx74n", feature = "ek-rx74m")))]
compile_error!("pick the board: --features ek-rx74n or ek-rx74m");

rx74_hal::bind_serial_interrupts!();
rx74_hal::bind_time_interrupts!();

const BANNER: &[u8] = b"serial-echo-async: rx74-hal AsyncSerial RSCI12, 115200 8N1\r\n";

/// Bytes received so far (to see from a debugger that it runs).
#[unsafe(no_mangle)]
static RX_BYTES: AtomicU32 = AtomicU32::new(0);
/// Receive errors so far.
#[unsafe(no_mangle)]
static RX_ERRORS: AtomicU32 = AtomicU32::new(0);
/// 100 ms ticks of the other task.
#[unsafe(no_mangle)]
static TICKS: AtomicU32 = AtomicU32::new(0);

fn error_text(e: Error) -> &'static [u8] {
    match e {
        Error::Overrun => b"\r\n! overrun\r\n",
        Error::Framing => b"\r\n! framing error\r\n",
        Error::Parity => b"\r\n! parity error\r\n",
    }
}

#[embassy_executor::task]
async fn echo(mut port: AsyncSerial) {
    port.write_all(BANNER).await.unwrap();
    let mut buf = [0u8; 64];
    loop {
        match port.read(&mut buf).await {
            Ok(n) => {
                RX_BYTES.fetch_add(n as u32, Ordering::Relaxed);
                port.write_all(&buf[..n]).await.unwrap();
            }
            Err(e) => {
                RX_ERRORS.fetch_add(1, Ordering::Relaxed);
                port.write_all(error_text(e)).await.unwrap();
            }
        }
    }
}

#[embassy_executor::task]
async fn ticker() {
    loop {
        Timer::after_millis(100).await;
        TICKS.fetch_add(1, Ordering::Relaxed);
    }
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = Config::pll_600mhz_from_24mhz().configure(&p.sysc, &p.sram, &p.mram);
    let pins = Parts::take().unwrap();
    let serial = Serial::rsci12(p.rsci12, pins.p114, pins.p115, 115_200, &clocks)
        .into_async(Priority::new(4).unwrap());
    rx74_hal::time::init(p.cmtw0, p.cmtw1, &clocks, Priority::new(5).unwrap());

    static mut EXECUTOR: MaybeUninit<Executor> = MaybeUninit::uninit();
    let executor = unsafe { (&raw mut EXECUTOR).as_mut().unwrap().write(Executor::new()) };
    executor.run(|spawner: Spawner| {
        spawner.spawn(echo(serial).unwrap());
        spawner.spawn(ticker().unwrap());
    })
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}
