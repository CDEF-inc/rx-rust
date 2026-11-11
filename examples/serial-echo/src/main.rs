//! Echoes what arrives on the AP-RX72M-0A's serial connector CN8 (SCI2,
//! P13 = TXD2 / P12 = RXD2, 115200 bps 8N1) back to the sender, forever. The
//! echo itself is written against the `embedded-io` traits only (`echo`);
//! `main` sets up rx72-hal's clock (PLL 240 MHz) and `Serial` and hands it
//! over. A receive error is reported as a line `! <error>` (a break gives a
//! framing error), and the echo goes on.
//!
//! To talk to it, connect a 3.3 V USB serial adapter to CN8 (its TXD to CN8
//! pin 1 = RXD2, its RXD to pin 2 = TXD2, and GND); scripts/hw/serial-echo.ps1
//! checks it from Windows. `RX_BYTES` / `RX_ERRORS` count what arrived.

#![no_std]
#![no_main]

use core::fmt::Display;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU32, Ordering};

use embedded_io::{Read, Write};
use rx72_hal::clock::Config;
use rx72_hal::gpio::Parts;
use rx72_hal::pac::Peripherals;
use rx72_hal::serial::Serial;
// The reset handler, vector tables and critical-section implementation.
use rx_rt as _;

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
    let p = Peripherals::take().unwrap();
    let clocks = Config::pll_240mhz_from_24mhz().configure(&p.system);
    let pins = Parts::take().unwrap();
    let mut serial = Serial::sci2(p.sci2, pins.p13, pins.p12, 115_200, &clocks);
    serial.write_all(b"serial-echo: rx72-hal SCI2, 115200 8N1\r\n").unwrap();
    echo(&mut serial)
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}
