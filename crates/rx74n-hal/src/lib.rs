//! embedded-hal 1.0 implementation for the Renesas RX74N (written for the
//! 320-pin R5K574NCB4AE of the EK-RX74N board).
//!
//! - [`clock`]: the clocks after a reset (the 8 MHz MOCO), and PLL1 from
//!   the main clock (CPUCLK0 480 or 600 MHz).
//! - [`gpio`]: port pins as `InputPin` / `OutputPin` / `StatefulOutputPin`,
//!   and the pin functions.
//! - [`delay`]: `DelayNs` on CMT2.
//! - [`serial`]: RSCI12 as a UART (`embedded-io` `Read` / `Write`).
//!
//! Everything runs in the root world, as a reset leaves it (a Flat-Project
//! in the hardware manual's terms), and uses the registers' secure
//! addresses. Register access goes through [`pac`] (rx74n-pac, generated
//! from e2 studio's RX74N.sfrx) where its types fit, and through the uniform
//! port register layout for GPIO. Read-modify-write of shared registers runs
//! in a critical section; the application provides the implementation
//! (rx-rt's `critical-section` feature).

#![no_std]

pub use embedded_hal;
pub use embedded_io;
pub use rx74n_pac as pac;

pub mod clock;
pub mod delay;
pub mod gpio;
mod mstp;
pub mod serial;
