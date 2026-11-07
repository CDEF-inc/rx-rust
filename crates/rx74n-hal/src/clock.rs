//! The clocks. A reset selects the 8 MHz MOCO for the system clock and for
//! SCICLK, with every divider at 1, and this crate leaves them so.

/// The clock frequencies in Hz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clocks {
    pub iclk: u32,
    pub pclkb: u32,
    /// The RSCI's operating clock (TCLK in the hardware manual).
    pub sciclk: u32,
}

impl Clocks {
    /// The clocks after a reset.
    pub const fn after_reset() -> Self {
        Clocks { iclk: 8_000_000, pclkb: 8_000_000, sciclk: 8_000_000 }
    }
}
