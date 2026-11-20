//! serial-echo with async tasks: echoes what arrives on the board's serial
//! port (115200 bps 8N1) through the HAL's interrupt-driven `AsyncSerial`
//! (`embedded-io-async`), while a second task counts 100 ms ticks on
//! embassy-time (the HAL's time driver on CMTW0 / CMTW1). Both run on
//! rx-rt's executor, which sleeps in `wait` while neither has work.
//!
//! The board is a feature (exactly one):
//!
//! - `ap-rx72m`: the AP-RX72M-0A's connector CN8 (SCI2, P13 / P12, a USB
//!   serial adapter as for serial-echo), with the PLL at 240 MHz;
//! - `ek-rx74n` / `ek-rx74m`: the USB serial port J27 (RSCI12, P114 / P115)
//!   with CPUCLK0 at 600 MHz.
//!
//! scripts/hw/serial-echo.ps1 checks it from Windows, as it does serial-echo.
//! `RX_BYTES` / `RX_ERRORS` count what arrived and `TICKS` the ticks.

#![no_std]
#![no_main]

use core::mem::MaybeUninit;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU32, Ordering};

use board::hal::serial::{AsyncSerial, Error};
use embassy_time::Timer;
use embedded_io_async::{Read, Write};
use rx_rt::executor::{Executor, Spawner};

#[cfg(not(any(feature = "ap-rx72m", feature = "ek-rx74n", feature = "ek-rx74m")))]
compile_error!("pick the board: --features ap-rx72m, ek-rx74n or ek-rx74m");
#[cfg(any(
    all(feature = "ap-rx72m", any(feature = "ek-rx74n", feature = "ek-rx74m")),
    all(feature = "ek-rx74n", feature = "ek-rx74m")
))]
compile_error!("pick one board only (ap-rx72m, ek-rx74n or ek-rx74m)");

/// The AP-RX72M-0A: SCI2 on CN8.
#[cfg(feature = "ap-rx72m")]
mod board {
    pub use rx72_hal as hal;
    use hal::icu::Priority;
    use hal::serial::{AsyncSerial, Serial};

    pub const BANNER: &[u8] = b"serial-echo-async: rx72-hal AsyncSerial SCI2, 115200 8N1\r\n";

    /// The serial port, with the time driver started.
    pub fn setup() -> AsyncSerial {
        let p = hal::pac::Peripherals::take().unwrap();
        let clocks = hal::clock::Config::pll_240mhz_from_24mhz().configure(&p.system);
        let pins = hal::gpio::Parts::take().unwrap();
        hal::time::init(p.cmtw0, p.cmtw1, &clocks, Priority::new(5).unwrap());
        Serial::sci2(p.sci2, pins.p13, pins.p12, 115_200, &clocks).into_async(Priority::new(4).unwrap())
    }
}

/// The EK-RX74N / EK-RX74M: RSCI12 on J27.
#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
mod board {
    pub use rx74_hal as hal;
    use hal::icu::Priority;
    use hal::serial::{AsyncSerial, Serial};

    pub const BANNER: &[u8] = b"serial-echo-async: rx74-hal AsyncSerial RSCI12, 115200 8N1\r\n";

    /// The serial port, with the time driver started.
    pub fn setup() -> AsyncSerial {
        let p = hal::pac::Peripherals::take().unwrap();
        let clocks = hal::clock::Config::pll_600mhz_from_24mhz().configure(&p.sysc, &p.sram, &p.mram);
        let pins = hal::gpio::Parts::take().unwrap();
        hal::time::init(p.cmtw0, p.cmtw1, &clocks, Priority::new(5).unwrap());
        Serial::rsci12(p.rsci12, pins.p114, pins.p115, 115_200, &clocks)
            .into_async(Priority::new(4).unwrap())
    }
}

board::hal::bind_serial_interrupts!();
board::hal::bind_time_interrupts!();

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
        // The error enums are non_exhaustive (Timeout is a write's).
        #[allow(unreachable_patterns)]
        _ => b"\r\n! error\r\n",
    }
}

#[embassy_executor::task]
async fn echo(mut port: AsyncSerial) {
    port.write_all(board::BANNER).await.unwrap();
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
    let serial = board::setup();
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
