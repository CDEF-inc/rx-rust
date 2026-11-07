//! Echoes what arrives on the EK-RX74N's USB serial port J27 (RSCI12, P114 =
//! TXD012-B / P115 = RXD012-B, 115200 bps 8N1) back to the sender, forever.
//! The echo itself is written against the `embedded-io` traits only (`echo`);
//! `main` sets up rx74n-hal's `Serial` and hands it over. A receive error is
//! reported as a line `! <error>` (a break gives a framing error), and the
//! echo goes on.
//!
//! scripts/hw/serial-echo.ps1 checks it from Windows. `RX_BYTES` /
//! `RX_ERRORS` count what arrived.

#![no_std]
#![no_main]

use core::fmt::Display;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU32, Ordering};

use embedded_io::{Read, Write};
use rx74n_hal::clock::Clocks;
use rx74n_hal::gpio::Parts;
use rx74n_hal::pac::Peripherals;
use rx74n_hal::serial::Serial;
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
    let clocks = Clocks::after_reset();
    let pins = Parts::take().unwrap();
    let mut serial = Serial::rsci12(p.rsci12, pins.p114, pins.p115, 115_200, &clocks);
    serial.write_all(b"serial-echo: rx74n-hal RSCI12, 115200 8N1\r\n").unwrap();
    echo(&mut serial)
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}
