//! Echoes what arrives on a board's serial port back to the sender, forever
//! (115200 bps 8N1). The board is a feature (exactly one):
//!
//! - `ap-rx72m`: the AP-RX72M-0A's connector CN8 (SCI2, P13 = TXD2 / P12 =
//!   RXD2), with rx72-hal's PLL at 240 MHz. Connect a 3.3 V USB serial
//!   adapter (its TXD to CN8 pin 1 = RXD2, its RXD to pin 2 = TXD2, and GND).
//! - `ek-rx74n` / `ek-rx74m`: the EK-RX74N's / EK-RX74M's USB serial port J27
//!   (RSCI12, P114 = TXD012-B / P115 = RXD012-B), with CPUCLK0 at 600 MHz
//!   (SCICLK stays on the 8 MHz MOCO).
//!
//! The echo itself is written against the `embedded-io` traits only
//! (`echo`); `board::serial` sets the clocks and the HAL's `Serial` up and
//! hands it over. A receive error is reported as a line `! <error>` (a break
//! gives a framing error), and the echo goes on.
//!
//! scripts/hw/serial-echo.ps1 checks it from Windows. `RX_BYTES` /
//! `RX_ERRORS` count what arrived.

#![no_std]
#![no_main]

use core::fmt::Display;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU32, Ordering};

use embedded_io::{Read, Write};
// The reset handler, vector tables and critical-section implementation.
use rx_rt as _;

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
    use rx72_hal::clock::Config;
    use rx72_hal::gpio::Parts;
    use rx72_hal::pac::Peripherals;
    use rx72_hal::serial::Serial;

    pub const BANNER: &[u8] = b"serial-echo: rx72-hal SCI2, 115200 8N1\r\n";

    pub fn serial() -> impl embedded_io::Read<Error: core::fmt::Display> + embedded_io::Write {
        let p = Peripherals::take().unwrap();
        let clocks = Config::pll_240mhz_from_24mhz().configure(&p.system);
        let pins = Parts::take().unwrap();
        Serial::sci2(p.sci2, pins.p13, pins.p12, 115_200, &clocks)
    }
}

/// The EK-RX74N / EK-RX74M: RSCI12 on J27.
#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
mod board {
    use rx74_hal::clock::Config;
    use rx74_hal::gpio::Parts;
    use rx74_hal::pac::Peripherals;
    use rx74_hal::serial::Serial;

    pub const BANNER: &[u8] = b"serial-echo: rx74-hal RSCI12, 115200 8N1, CPUCLK0 600 MHz\r\n";

    pub fn serial() -> impl embedded_io::Read<Error: core::fmt::Display> + embedded_io::Write {
        let p = Peripherals::take().unwrap();
        let clocks = Config::pll_600mhz_from_24mhz().configure(&p.sysc, &p.sram, &p.mram);
        let pins = Parts::take().unwrap();
        Serial::rsci12(p.rsci12, pins.p114, pins.p115, 115_200, &clocks)
    }
}

/// Bytes received so far (to see from a debugger that it runs).
#[unsafe(no_mangle)]
static RX_BYTES: AtomicU32 = AtomicU32::new(0);
/// Receive errors so far.
#[unsafe(no_mangle)]
static RX_ERRORS: AtomicU32 = AtomicU32::new(0);

/// Writes back what `port` reads, as it arrives; forever.
fn echo<S>(port: &mut S) -> !
where
    S: Read + Write,
    S::Error: Display,
{
    let mut buf = [0u8; 64];
    loop {
        match port.read(&mut buf) {
            Ok(n) => {
                RX_BYTES.fetch_add(n as u32, Ordering::Relaxed);
                port.write_all(&buf[..n]).unwrap();
            }
            Err(e) => {
                RX_ERRORS.fetch_add(1, Ordering::Relaxed);
                write!(port, "\r\n! {e}\r\n").unwrap();
            }
        }
    }
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let mut serial = board::serial();
    serial.write_all(board::BANNER).unwrap();
    echo(&mut serial)
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}
