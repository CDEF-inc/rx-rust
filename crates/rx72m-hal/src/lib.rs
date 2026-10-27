//! embedded-hal 1.0 implementation for the Renesas RX72M (written for the
//! 176-pin R5F572MNDDFC of the AP-RX72M-0A board).
//!
//! - [`clock`]: the main oscillator and the PLL (ICLK up to 240 MHz).
//! - [`gpio`]: port pins as `InputPin` / `OutputPin` / `StatefulOutputPin`.
//!
//! Register access goes through [`pac`] (rx72m-pac, generated from e2
//! studio's RX72M.sfrx) where its types fit, and through the uniform port
//! register layout for GPIO. Read-modify-write of shared registers runs in a
//! critical section; the application provides the implementation (rx-rt's
//! `critical-section` feature).

#![no_std]

pub use embedded_hal;
pub use rx72m_pac as pac;

pub mod clock;
pub mod gpio;
