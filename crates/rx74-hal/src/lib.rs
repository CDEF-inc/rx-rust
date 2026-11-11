//! embedded-hal 1.0 implementation for the Renesas RX74M and RX74N (written
//! for the 320-pin R5K574NCB4AE of the EK-RX74N board, and run as it is on
//! the EK-RX74M). The feature `rx74m` or `rx74n` (exactly one) picks the
//! chip; the two differ only in the RX74M's EtherCAT slave controller and
//! DSMIF, which this crate does not use (see rx74-pac).
//!
//! - [`clock`]: the clocks after a reset (the 8 MHz MOCO), and PLL1 from
//!   the main clock (CPUCLK0 480 or 600 MHz).
//! - [`gpio`]: port pins as `InputPin` / `OutputPin` / `StatefulOutputPin`,
//!   and the pin functions.
//! - [`delay`]: `DelayNs` on CMT2.
//! - [`serial`]: RSCI12 as a UART (`embedded-io` `Read` / `Write`).
//! - [`spi`]: RSPIA0 as an SPI master (`SpiBus`).
//! - [`pwm`]: GPTW8 PWM on PF05 (the LED LED1) as `SetDutyCycle`.
//! - [`i2c`]: RI3C0 as an I2C master (`I2c`) on P102 / P103.
//! - [`adc`]: the 16-bit A/D converter (unit 0, SAR) and the temperature
//!   sensor.
//!
//! Everything runs in the root world, as a reset leaves it (a Flat-Project
//! in the hardware manual's terms), and uses the registers' secure
//! addresses. Register access goes through [`pac`] (rx74-pac, generated
//! from e2 studio's RX74M.sfrx / RX74N.sfrx) where its types fit, and directly by address
//! for the uniform port registers, RSPIA0, GPTW8, ADC16H and RI3C0.
//! Read-modify-write of shared registers runs in a critical section; the
//! application provides the implementation (rx-rt's `critical-section`
//! feature).

#![no_std]

pub use embedded_hal;
pub use embedded_io;
pub use rx74_pac as pac;

pub mod adc;
pub mod clock;
pub mod delay;
pub mod gpio;
pub mod i2c;
mod mstp;
pub mod pwm;
pub mod serial;
pub mod spi;
