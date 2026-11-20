//! embedded-hal 1.0 implementation for the Renesas RX72M and RX72N (written
//! for the 176-pin R5F572MNDDFC of the AP-RX72M-0A board). The feature
//! `rx72m` or `rx72n` (exactly one) picks the chip; the two differ only in
//! the RX72M's EtherCAT slave controller and DSMIF, which this crate does
//! not use (see rx72-pac).
//!
//! - [`clock`]: the main oscillator and the PLL (ICLK up to 240 MHz).
//! - [`gpio`]: port pins as `InputPin` / `OutputPin` / `StatefulOutputPin`,
//!   and the MPC pin functions.
//! - [`delay`]: `DelayNs` on CMT2.
//! - [`icu`]: the interrupt controller (enable, priority and request flag
//!   per source, the software interrupts).
//! - [`serial`]: SCI2 as a UART (`embedded-io` `Read` / `Write`; with the
//!   feature `async`, interrupt-driven `embedded-io-async` too).
//! - [`spi`]: RSPI2 as an SPI master (`SpiBus`).
//! - [`adc`]: the 12-bit A/D converter units, the internal reference
//!   voltage and the temperature sensor.
//! - [`pwm`]: MTU6 PWM on PJ1 (the LED LD2) as `SetDutyCycle`.
//! - [`i2c`]: RIIC0 as an I2C master (`I2c`) on P12 / P13.
//! - [`sci_i2c`]: SCI6 in simple I2C mode (`I2c`) on P00 / P01, the
//!   board's EEPROM.
//! - `time` (feature `embassy-time-driver`): an embassy-time driver on
//!   CMTW0 / CMTW1.
//!
//! Register access goes through [`pac`] (rx72-pac, generated from e2
//! studio's RX72M.sfrx / RX72N.sfrx) where its types fit, and directly by address for
//! the uniform port registers, the A/D converter units and MTU6.
//! Read-modify-write of shared registers runs in a critical section; the
//! application provides the implementation (rx-rt's `critical-section`
//! feature).

#![no_std]

pub use embedded_hal;
pub use embedded_io;
pub use rx72_pac as pac;

pub mod adc;
pub mod clock;
pub mod delay;
pub mod gpio;
pub mod i2c;
pub mod icu;
mod mstp;
pub mod pwm;
pub mod sci_i2c;
pub mod serial;
pub mod spi;
#[cfg(feature = "embassy-time-driver")]
pub mod time;
