//! Bounded waits, and the errors of the `try_*` constructors.

use core::fmt;

/// A wait for the hardware that did not end in time: a clock that does not
/// start, a peripheral that does not finish (a stuck bus, a fault).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeout;

impl fmt::Display for Timeout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("timeout")
    }
}

impl core::error::Error for Timeout {}

/// A setting the hardware cannot make (a baud rate, a frequency, a clock
/// ratio out of range), from a `try_*` constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfRange;

impl fmt::Display for OutOfRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("setting out of range")
    }
}

impl core::error::Error for OutOfRange {}

/// Loop iterations for a wait of at least `us` microseconds with the CPU
/// at `cpu_hz` (CPUCLK0): each iteration takes at least one CPU cycle, so
/// the bound is never shorter than `us` (in practice a few times longer).
pub(crate) const fn spins(cpu_hz: u32, us: u32) -> u32 {
    let n = cpu_hz as u64 * us as u64 / 1_000_000;
    if n > u32::MAX as u64 { u32::MAX } else if n == 0 { 1 } else { n as u32 }
}

/// Calls `done` until it returns true or `spins` iterations have passed.
pub(crate) fn wait(spins: u32, mut done: impl FnMut() -> bool) -> Result<(), Timeout> {
    let mut left = spins;
    loop {
        if done() {
            return Ok(());
        }
        if left == 0 {
            return Err(Timeout);
        }
        left -= 1;
    }
}
