//! The hw-pwm board test: rx72.rs on the AP-RX72M-0A (feature ap-rx72m),
//! rx74.rs on the EK-RX74N / EK-RX74M (ek-rx74n / ek-rx74m). hw-board has
//! the result variables and the end of the test.

#![no_std]
#![no_main]
// The result variables keep the names the test scripts read.
#![allow(non_upper_case_globals)]

#[cfg_attr(feature = "ap-rx72m", path = "rx72.rs")]
#[cfg_attr(any(feature = "ek-rx74n", feature = "ek-rx74m"), path = "rx74.rs")]
mod board;
