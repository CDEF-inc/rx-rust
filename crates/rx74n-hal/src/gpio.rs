//! Port pins as embedded-hal 1.0 digital pins.
//!
//! [`Parts::take`] gives every port pin of the 320-pin RX74N (ports 0-9 and
//! A-G, up to 16 pins each) once, as a [`Pin`] in [`Input`] mode, the state
//! after reset (except P302, the JTAG TDO). A pin changes mode with
//! `into_*`: [`Output`] (push-pull), [`Input`] or [`PullUpInput`], all
//! general I/O port modes, or [`Function`] (`into_function(psel)`) for a
//! peripheral. P300-P303 carry JTAG: changing them cuts off the debugger.
//!
//! A pin's mode is all in its PmnPFS register (at 0x87C00800 + 0x40 * port
//! + 4 * pin), behind the PWPR_S write protection. The data registers have
//! the same layout for every port (0x87C00000 + 0x20 * port: PODR + 2, PIDR
//! + 4, 16 bits). Changes run in a critical section.

use core::convert::Infallible;
use core::marker::PhantomData;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};

use embedded_hal::digital::{ErrorType, InputPin, OutputPin, StatefulOutputPin};

const PORT: usize = 0x87c0_0000;
const PODR: usize = 2;
const PIDR: usize = 4;
const PFS: usize = 0x87c0_0800;
const PWPR_S: *mut u8 = 0x87c0_0d14_usize as _;

// PmnPFS bits
const PFS_PODR: u32 = 1 << 0;
const PFS_PDR: u32 = 1 << 2;
const PFS_PCR: u32 = 1 << 4;
const PFS_ASEL: u32 = 1 << 15;
const PFS_PMR: u32 = 1 << 16;
const PFS_PSEL_SHIFT: u32 = 24;
const PFS_PSEL: u32 = 0x7f << PFS_PSEL_SHIFT;

/// Input (PDR = 0, no pull-up).
pub struct Input;
/// Input with the internal pull-up (PDR = 0, PCR = 1).
pub struct PullUpInput;
/// Push-pull output (PDR = 1).
pub struct Output;
/// A peripheral function (PMR = 1, the function chosen by PSEL).
pub struct Function;
/// An analog input (PMR = 0, PDR = 0, PCR = 0, ASEL = 1). The pin is not
/// read through PIDR then.
pub struct Analog;

/// Implemented (for `()`) for the pins whose analog input the drivers in
/// this crate use: P400-P407 (AN000-AN007, the A/D converter's unit 0).
pub trait AnalogCapable<const P: usize, const N: u8> {}

/// Pin `N` of port `P` (port numbers as in the register addresses: 0-9,
/// 0x0A-0x10 for A-G).
pub struct Pin<const P: usize, const N: u8, MODE> {
    _mode: PhantomData<MODE>,
}

fn data(p: usize, reg: usize) -> *mut u16 {
    (PORT + 0x20 * p + reg) as _
}

fn pfs(p: usize, n: u8) -> *mut u32 {
    (PFS + 0x40 * p + 4 * n as usize) as _
}

/// Writes PmnPFS with what `f` makes of it, through the write protection:
/// B0WI = 0, PFSWE = 1, then back.
fn update_pfs(p: usize, n: u8, f: impl FnOnce(u32) -> u32) {
    critical_section::with(|_| unsafe {
        write_volatile(PWPR_S, 0x00);
        write_volatile(PWPR_S, 0x40);
        let r = pfs(p, n);
        write_volatile(r, f(read_volatile(r)));
        write_volatile(PWPR_S, 0x00);
        write_volatile(PWPR_S, 0x80);
    });
}

impl<const P: usize, const N: u8, MODE> Pin<P, N, MODE> {
    const fn new() -> Self {
        Pin { _mode: PhantomData }
    }

    fn pin_is_high(&self) -> bool {
        unsafe { read_volatile(data(P, PIDR)) & 1 << N != 0 }
    }

    /// Push-pull output, starting at `high`.
    pub fn into_output(self, high: bool) -> Pin<P, N, Output> {
        update_pfs(P, N, |v| {
            let v = v & !(PFS_PMR | PFS_PCR | PFS_PODR | PFS_ASEL);
            v | PFS_PDR | if high { PFS_PODR } else { 0 }
        });
        Pin::new()
    }

    /// Push-pull output, starting low.
    pub fn into_push_pull_output(self) -> Pin<P, N, Output> {
        self.into_output(false)
    }

    /// Input without the pull-up.
    pub fn into_input(self) -> Pin<P, N, Input> {
        update_pfs(P, N, |v| v & !(PFS_PMR | PFS_PDR | PFS_PCR | PFS_ASEL));
        Pin::new()
    }

    /// Input with the internal pull-up.
    pub fn into_pull_up_input(self) -> Pin<P, N, PullUpInput> {
        update_pfs(P, N, |v| v & !(PFS_PMR | PFS_PDR | PFS_ASEL) | PFS_PCR);
        Pin::new()
    }

    /// A peripheral function: PMR = 0, PSEL = `psel`, PMR = 1. The PSEL
    /// values are in the hardware manual's I/O ports chapter; the drivers
    /// in this crate set their own pins.
    pub fn into_function(self, psel: u8) -> Pin<P, N, Function> {
        assert!(psel < 128);
        update_pfs(P, N, |v| v & !(PFS_PMR | PFS_ASEL));
        update_pfs(P, N, |v| v & !PFS_PSEL | (psel as u32) << PFS_PSEL_SHIFT);
        update_pfs(P, N, |v| v | PFS_PMR);
        Pin::new()
    }
}

impl<const P: usize, const N: u8, MODE> Pin<P, N, MODE>
where
    (): AnalogCapable<P, N>,
{
    /// An analog input: PMR = 0, PDR = 0, PCR = 0, then ASEL = 1, as the I/O
    /// ports chapter asks for analog pins. The other mode changes clear
    /// ASEL again.
    pub fn into_analog(self) -> Pin<P, N, Analog> {
        update_pfs(P, N, |v| v & !(PFS_PMR | PFS_PDR | PFS_PCR));
        update_pfs(P, N, |v| v | PFS_ASEL);
        Pin::new()
    }
}

impl<const P: usize, const N: u8> Pin<P, N, Function> {
    /// The PSEL of the pin.
    pub fn psel(&self) -> u8 {
        unsafe { ((read_volatile(pfs(P, N)) & PFS_PSEL) >> PFS_PSEL_SHIFT) as u8 }
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

fn set_podr(p: usize, n: u8, high: bool) {
    critical_section::with(|_| unsafe {
        let r = data(p, PODR);
        let v = read_volatile(r);
        write_volatile(r, if high { v | 1 << n } else { v & !(1 << n) });
    });
}

impl<const P: usize, const N: u8> OutputPin for Pin<P, N, Output> {
    fn set_low(&mut self) -> Result<(), Infallible> {
        set_podr(P, N, false);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        set_podr(P, N, true);
        Ok(())
    }
}

/// The level the pin drives (PODR).
impl<const P: usize, const N: u8> StatefulOutputPin for Pin<P, N, Output> {
    fn is_set_high(&mut self) -> Result<bool, Infallible> {
        Ok(unsafe { read_volatile(data(P, PODR)) } & 1 << N != 0)
    }

    fn is_set_low(&mut self) -> Result<bool, Infallible> {
        Ok(unsafe { read_volatile(data(P, PODR)) } & 1 << N == 0)
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
    p000: 0x00, 0;
    p001: 0x00, 1;
    p002: 0x00, 2;
    p003: 0x00, 3;
    p004: 0x00, 4;
    p005: 0x00, 5;
    p006: 0x00, 6;
    p007: 0x00, 7;
    p008: 0x00, 8;
    p009: 0x00, 9;
    p010: 0x00, 10;
    p011: 0x00, 11;
    p012: 0x00, 12;
    p013: 0x00, 13;
    p014: 0x00, 14;
    p015: 0x00, 15;
    p100: 0x01, 0;
    p101: 0x01, 1;
    p102: 0x01, 2;
    p103: 0x01, 3;
    p104: 0x01, 4;
    p105: 0x01, 5;
    p106: 0x01, 6;
    p107: 0x01, 7;
    p108: 0x01, 8;
    p109: 0x01, 9;
    p110: 0x01, 10;
    p111: 0x01, 11;
    p112: 0x01, 12;
    p113: 0x01, 13;
    p114: 0x01, 14;
    p115: 0x01, 15;
    p200: 0x02, 0;
    p201: 0x02, 1;
    p202: 0x02, 2;
    p203: 0x02, 3;
    p204: 0x02, 4;
    p205: 0x02, 5;
    p206: 0x02, 6;
    p207: 0x02, 7;
    p208: 0x02, 8;
    p209: 0x02, 9;
    p210: 0x02, 10;
    p211: 0x02, 11;
    p212: 0x02, 12;
    p213: 0x02, 13;
    p214: 0x02, 14;
    p215: 0x02, 15;
    p300: 0x03, 0;
    p301: 0x03, 1;
    p302: 0x03, 2;
    p303: 0x03, 3;
    p304: 0x03, 4;
    p305: 0x03, 5;
    p306: 0x03, 6;
    p307: 0x03, 7;
    p308: 0x03, 8;
    p309: 0x03, 9;
    p310: 0x03, 10;
    p311: 0x03, 11;
    p312: 0x03, 12;
    p315: 0x03, 15;
    p400: 0x04, 0;
    p401: 0x04, 1;
    p402: 0x04, 2;
    p403: 0x04, 3;
    p404: 0x04, 4;
    p405: 0x04, 5;
    p406: 0x04, 6;
    p407: 0x04, 7;
    p408: 0x04, 8;
    p409: 0x04, 9;
    p410: 0x04, 10;
    p411: 0x04, 11;
    p412: 0x04, 12;
    p413: 0x04, 13;
    p414: 0x04, 14;
    p415: 0x04, 15;
    p500: 0x05, 0;
    p501: 0x05, 1;
    p502: 0x05, 2;
    p503: 0x05, 3;
    p504: 0x05, 4;
    p505: 0x05, 5;
    p506: 0x05, 6;
    p507: 0x05, 7;
    p508: 0x05, 8;
    p509: 0x05, 9;
    p510: 0x05, 10;
    p511: 0x05, 11;
    p512: 0x05, 12;
    p513: 0x05, 13;
    p514: 0x05, 14;
    p515: 0x05, 15;
    p600: 0x06, 0;
    p601: 0x06, 1;
    p602: 0x06, 2;
    p603: 0x06, 3;
    p604: 0x06, 4;
    p605: 0x06, 5;
    p606: 0x06, 6;
    p607: 0x06, 7;
    p700: 0x07, 0;
    p701: 0x07, 1;
    p702: 0x07, 2;
    p703: 0x07, 3;
    p704: 0x07, 4;
    p705: 0x07, 5;
    p706: 0x07, 6;
    p707: 0x07, 7;
    p708: 0x07, 8;
    p709: 0x07, 9;
    p710: 0x07, 10;
    p711: 0x07, 11;
    p712: 0x07, 12;
    p713: 0x07, 13;
    p714: 0x07, 14;
    p715: 0x07, 15;
    p800: 0x08, 0;
    p801: 0x08, 1;
    p802: 0x08, 2;
    p803: 0x08, 3;
    p804: 0x08, 4;
    p805: 0x08, 5;
    p806: 0x08, 6;
    p807: 0x08, 7;
    p900: 0x09, 0;
    p901: 0x09, 1;
    p902: 0x09, 2;
    p903: 0x09, 3;
    p904: 0x09, 4;
    p905: 0x09, 5;
    p906: 0x09, 6;
    p907: 0x09, 7;
    pa00: 0x0a, 0;
    pa01: 0x0a, 1;
    pa02: 0x0a, 2;
    pa03: 0x0a, 3;
    pa04: 0x0a, 4;
    pa05: 0x0a, 5;
    pa06: 0x0a, 6;
    pa07: 0x0a, 7;
    pb00: 0x0b, 0;
    pb01: 0x0b, 1;
    pb02: 0x0b, 2;
    pb03: 0x0b, 3;
    pb04: 0x0b, 4;
    pb05: 0x0b, 5;
    pb06: 0x0b, 6;
    pb07: 0x0b, 7;
    pc00: 0x0c, 0;
    pc01: 0x0c, 1;
    pc02: 0x0c, 2;
    pc03: 0x0c, 3;
    pc04: 0x0c, 4;
    pc05: 0x0c, 5;
    pc06: 0x0c, 6;
    pc07: 0x0c, 7;
    pd00: 0x0d, 0;
    pd01: 0x0d, 1;
    pd02: 0x0d, 2;
    pd03: 0x0d, 3;
    pd04: 0x0d, 4;
    pd05: 0x0d, 5;
    pd06: 0x0d, 6;
    pd07: 0x0d, 7;
    pe00: 0x0e, 0;
    pe01: 0x0e, 1;
    pe02: 0x0e, 2;
    pe03: 0x0e, 3;
    pe04: 0x0e, 4;
    pe05: 0x0e, 5;
    pe06: 0x0e, 6;
    pe07: 0x0e, 7;
    pf00: 0x0f, 0;
    pf01: 0x0f, 1;
    pf02: 0x0f, 2;
    pf03: 0x0f, 3;
    pf04: 0x0f, 4;
    pf05: 0x0f, 5;
    pf06: 0x0f, 6;
    pf07: 0x0f, 7;
    pf08: 0x0f, 8;
    pf09: 0x0f, 9;
    pg00: 0x10, 0;
    pg01: 0x10, 1;
    pg02: 0x10, 2;
    pg03: 0x10, 3;
    pg04: 0x10, 4;
    pg05: 0x10, 5;
    pg06: 0x10, 6;
    pg07: 0x10, 7;
    pg08: 0x10, 8;
    pg09: 0x10, 9;
    pg10: 0x10, 10;
    pg11: 0x10, 11;
    pg12: 0x10, 12;
    pg13: 0x10, 13;
    pg14: 0x10, 14;
    pg15: 0x10, 15;
}

// AN000-AN007
impl AnalogCapable<0x04, 0> for () {}
impl AnalogCapable<0x04, 1> for () {}
impl AnalogCapable<0x04, 2> for () {}
impl AnalogCapable<0x04, 3> for () {}
impl AnalogCapable<0x04, 4> for () {}
impl AnalogCapable<0x04, 5> for () {}
impl AnalogCapable<0x04, 6> for () {}
impl AnalogCapable<0x04, 7> for () {}
