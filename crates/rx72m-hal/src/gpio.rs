//! Port pins as embedded-hal 1.0 digital pins.
//!
//! [`Parts::take`] gives every port pin of the 176-pin RX72M (the pins in the
//! AP-RX72M-0A manual's I/O tables: ports 0-9, A-G and J) once, as a
//! [`Pin`] in [`Input`] mode, the state after reset. A pin changes mode with
//! `into_*`: [`Output`] (push-pull, PDR = 1), [`Input`] or [`PullUpInput`]
//! (PCR = 1). Every mode is a general I/O port (PMR = 0); the peripheral
//! functions (MPC PFS) are for the drivers that need them.
//!
//! The port registers have the same layout for every port (PDR at
//! 0x8C000 + port, PODR + 0x20, PIDR + 0x40, PMR + 0x60, PCR + 0xC0), so the
//! pins use that layout directly. Bits of the same port register are
//! changed with read-modify-write in a critical section.

use core::convert::Infallible;
use core::marker::PhantomData;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};

use embedded_hal::digital::{ErrorType, InputPin, OutputPin, StatefulOutputPin};

const PDR: usize = 0x0008_c000;
const PODR: usize = 0x0008_c020;
const PIDR: usize = 0x0008_c040;
const PMR: usize = 0x0008_c060;
const PCR: usize = 0x0008_c0c0;

/// Input (PDR = 0, no pull-up).
pub struct Input;
/// Input with the internal pull-up (PDR = 0, PCR = 1).
pub struct PullUpInput;
/// Push-pull output (PDR = 1).
pub struct Output;

/// Pin `N` of port `P` (port numbers as in the register addresses: 0-9,
/// 0x0A-0x10 for A-G, 0x12 for J).
pub struct Pin<const P: usize, const N: u8, MODE> {
    _mode: PhantomData<MODE>,
}

fn read(base: usize, p: usize) -> u8 {
    unsafe { read_volatile((base + p) as *const u8) }
}

fn modify(base: usize, p: usize, bit: u8, set: bool) {
    critical_section::with(|_| unsafe {
        let r = (base + p) as *mut u8;
        let v = read_volatile(r);
        write_volatile(r, if set { v | 1 << bit } else { v & !(1 << bit) });
    });
}

impl<const P: usize, const N: u8, MODE> Pin<P, N, MODE> {
    const fn new() -> Self {
        Pin { _mode: PhantomData }
    }

    /// Push-pull output, starting at `high` (PODR is set before PDR).
    pub fn into_output(self, high: bool) -> Pin<P, N, Output> {
        modify(PMR, P, N, false);
        modify(PCR, P, N, false);
        modify(PODR, P, N, high);
        modify(PDR, P, N, true);
        Pin::new()
    }

    /// Push-pull output, starting low.
    pub fn into_push_pull_output(self) -> Pin<P, N, Output> {
        self.into_output(false)
    }

    /// Input without the pull-up.
    pub fn into_input(self) -> Pin<P, N, Input> {
        modify(PMR, P, N, false);
        modify(PDR, P, N, false);
        modify(PCR, P, N, false);
        Pin::new()
    }

    /// Input with the internal pull-up.
    pub fn into_pull_up_input(self) -> Pin<P, N, PullUpInput> {
        modify(PMR, P, N, false);
        modify(PDR, P, N, false);
        modify(PCR, P, N, true);
        Pin::new()
    }

    fn pin_is_high(&self) -> bool {
        read(PIDR, P) & 1 << N != 0
    }
}

impl<const P: usize, const N: u8, MODE> ErrorType for Pin<P, N, MODE> {
    type Error = Infallible;
}

/// The level at the pin (PIDR), in any mode.
impl<const P: usize, const N: u8, MODE> InputPin for Pin<P, N, MODE> {
    fn is_high(&mut self) -> Result<bool, Infallible> {
        Ok(self.pin_is_high())
    }

    fn is_low(&mut self) -> Result<bool, Infallible> {
        Ok(!self.pin_is_high())
    }
}

impl<const P: usize, const N: u8> OutputPin for Pin<P, N, Output> {
    fn set_low(&mut self) -> Result<(), Infallible> {
        modify(PODR, P, N, false);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        modify(PODR, P, N, true);
        Ok(())
    }
}

/// The level the pin drives (PODR).
impl<const P: usize, const N: u8> StatefulOutputPin for Pin<P, N, Output> {
    fn is_set_high(&mut self) -> Result<bool, Infallible> {
        Ok(read(PODR, P) & 1 << N != 0)
    }

    fn is_set_low(&mut self) -> Result<bool, Infallible> {
        Ok(read(PODR, P) & 1 << N == 0)
    }
}

macro_rules! parts {
    ($($name:ident: $port:expr, $bit:expr;)*) => {
        /// Every port pin, in [`Input`] mode.
        #[allow(missing_docs)]
        pub struct Parts {
            $(pub $name: Pin<$port, $bit, Input>,)*
        }

        impl Parts {
            /// The pins, once (`None` after the first call).
            pub fn take() -> Option<Self> {
                static TAKEN: AtomicBool = AtomicBool::new(false);
                let taken = critical_section::with(|_| {
                    let t = TAKEN.load(Ordering::Relaxed);
                    TAKEN.store(true, Ordering::Relaxed);
                    t
                });
                if taken {
                    None
                } else {
                    Some(Parts { $($name: Pin::new(),)* })
                }
            }
        }
    };
}

parts! {
    // P0
    p00: 0x00, 0;
    p01: 0x00, 1;
    p02: 0x00, 2;
    p03: 0x00, 3;
    p05: 0x00, 5;
    p07: 0x00, 7;
    // P1
    p10: 0x01, 0;
    p11: 0x01, 1;
    p12: 0x01, 2;
    p13: 0x01, 3;
    p14: 0x01, 4;
    p15: 0x01, 5;
    p16: 0x01, 6;
    p17: 0x01, 7;
    // P2
    p20: 0x02, 0;
    p21: 0x02, 1;
    p22: 0x02, 2;
    p23: 0x02, 3;
    p24: 0x02, 4;
    p25: 0x02, 5;
    p26: 0x02, 6;
    p27: 0x02, 7;
    // P3
    p30: 0x03, 0;
    p31: 0x03, 1;
    p32: 0x03, 2;
    p33: 0x03, 3;
    p34: 0x03, 4;
    p35: 0x03, 5;
    p36: 0x03, 6;
    p37: 0x03, 7;
    // P4
    p40: 0x04, 0;
    p41: 0x04, 1;
    p42: 0x04, 2;
    p43: 0x04, 3;
    p44: 0x04, 4;
    p45: 0x04, 5;
    p46: 0x04, 6;
    p47: 0x04, 7;
    // P5
    p50: 0x05, 0;
    p51: 0x05, 1;
    p52: 0x05, 2;
    p53: 0x05, 3;
    p54: 0x05, 4;
    p55: 0x05, 5;
    p56: 0x05, 6;
    p57: 0x05, 7;
    // P6
    p60: 0x06, 0;
    p61: 0x06, 1;
    p62: 0x06, 2;
    p63: 0x06, 3;
    p64: 0x06, 4;
    p65: 0x06, 5;
    p66: 0x06, 6;
    p67: 0x06, 7;
    // P7
    p70: 0x07, 0;
    p71: 0x07, 1;
    p72: 0x07, 2;
    p73: 0x07, 3;
    p74: 0x07, 4;
    p75: 0x07, 5;
    p76: 0x07, 6;
    p77: 0x07, 7;
    // P8
    p80: 0x08, 0;
    p81: 0x08, 1;
    p82: 0x08, 2;
    p83: 0x08, 3;
    p84: 0x08, 4;
    p85: 0x08, 5;
    p86: 0x08, 6;
    p87: 0x08, 7;
    // P9
    p90: 0x09, 0;
    p91: 0x09, 1;
    p92: 0x09, 2;
    p93: 0x09, 3;
    p94: 0x09, 4;
    p95: 0x09, 5;
    p96: 0x09, 6;
    p97: 0x09, 7;
    // PA
    pa0: 0x0a, 0;
    pa1: 0x0a, 1;
    pa2: 0x0a, 2;
    pa3: 0x0a, 3;
    pa4: 0x0a, 4;
    pa5: 0x0a, 5;
    pa6: 0x0a, 6;
    pa7: 0x0a, 7;
    // PB
    pb0: 0x0b, 0;
    pb1: 0x0b, 1;
    pb2: 0x0b, 2;
    pb3: 0x0b, 3;
    pb4: 0x0b, 4;
    pb5: 0x0b, 5;
    pb6: 0x0b, 6;
    pb7: 0x0b, 7;
    // PC
    pc0: 0x0c, 0;
    pc1: 0x0c, 1;
    pc2: 0x0c, 2;
    pc3: 0x0c, 3;
    pc4: 0x0c, 4;
    pc5: 0x0c, 5;
    pc6: 0x0c, 6;
    pc7: 0x0c, 7;
    // PD
    pd0: 0x0d, 0;
    pd1: 0x0d, 1;
    pd2: 0x0d, 2;
    pd3: 0x0d, 3;
    pd4: 0x0d, 4;
    pd5: 0x0d, 5;
    pd6: 0x0d, 6;
    pd7: 0x0d, 7;
    // PE
    pe0: 0x0e, 0;
    pe1: 0x0e, 1;
    pe2: 0x0e, 2;
    pe3: 0x0e, 3;
    pe4: 0x0e, 4;
    pe5: 0x0e, 5;
    pe6: 0x0e, 6;
    pe7: 0x0e, 7;
    // PF
    pf0: 0x0f, 0;
    pf1: 0x0f, 1;
    pf2: 0x0f, 2;
    pf3: 0x0f, 3;
    pf4: 0x0f, 4;
    pf5: 0x0f, 5;
    // PG
    pg0: 0x10, 0;
    pg1: 0x10, 1;
    pg2: 0x10, 2;
    pg3: 0x10, 3;
    pg4: 0x10, 4;
    pg5: 0x10, 5;
    pg6: 0x10, 6;
    pg7: 0x10, 7;
    // PJ
    pj0: 0x12, 0;
    pj1: 0x12, 1;
    pj2: 0x12, 2;
    pj3: 0x12, 3;
    pj5: 0x12, 5;
}
