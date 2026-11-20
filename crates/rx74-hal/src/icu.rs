//! The interrupt controller (ICU, at its secure addresses): per-source
//! enable (IERm.IENj), priority (IPRr) and request flag (IRn), and the two
//! software interrupts.
//!
//! An [`Interrupt`] is a request source: its vector number (n, as in
//! rx-rt's `interrupt!(n, handler)`) and its priority register. Vectors
//! below 128 do not all have IPRn for vector n: SWINT (27) has IPR3 and
//! SWINT2 (26) IPR8, as in the manual's interrupt vector table (the RX72M
//! gives both IPR3). The constants below carry the right one;
//! [`Interrupt::new`] takes both numbers for a source that has no constant
//! here.
//!
//! A source is accepted when its IR is set, its IEN is set, its priority
//! is above PSW.IPL and PSW.I is set (rx-rt's `interrupt::enable`). A
//! priority of 0 means the request is never accepted, so set one before
//! enabling. Enabling or disabling a source does not affect critical
//! sections, which clear PSW.I.

use core::ptr::{read_volatile, write_volatile};

const ICU: usize = 0x8700_6000;
const IR: usize = ICU;
const IER: usize = ICU + 0x200;
const SWINTR: *mut u8 = (ICU + 0x2e0) as _;
const SWINT2R: *mut u8 = (ICU + 0x2e1) as _;
const IPR: usize = ICU + 0x300;

/// An interrupt request source: its vector number and priority register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interrupt {
    vector: u8,
    ipr: u8,
}

/// An interrupt priority level, 1 (lowest) to 15 (highest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(u8);

impl Priority {
    pub const P1: Priority = Priority(1);
    pub const P2: Priority = Priority(2);
    pub const P7: Priority = Priority(7);
    pub const P15: Priority = Priority(15);

    /// The level `level`, or `None` unless it is 1 to 15.
    pub const fn new(level: u8) -> Option<Priority> {
        if level >= 1 && level <= 15 { Some(Priority(level)) } else { None }
    }

    /// The level, 1 to 15.
    pub const fn level(self) -> u8 {
        self.0
    }
}

impl Interrupt {
    /// SWINT2 (software interrupt 2), set by [`trigger_swint2`].
    pub const SWINT2: Interrupt = Interrupt { vector: 26, ipr: 8 };
    /// SWINT (software interrupt), set by [`trigger_swint`].
    pub const SWINT: Interrupt = Interrupt { vector: 27, ipr: 3 };
    /// CMT0 compare match.
    pub const CMI0: Interrupt = Interrupt { vector: 28, ipr: 4 };
    /// CMT1 compare match.
    pub const CMI1: Interrupt = Interrupt { vector: 29, ipr: 5 };
    /// CMTW0 compare match.
    pub const CMWI0: Interrupt = Interrupt { vector: 30, ipr: 6 };
    /// CMTW1 compare match.
    pub const CMWI1: Interrupt = Interrupt { vector: 31, ipr: 7 };

    /// A source with no constant here: vector `vector` and priority
    /// register IPR`ipr`, from the manual's interrupt vector table.
    ///
    /// # Safety
    ///
    /// `ipr` must be the source's priority register: the methods write it,
    /// and a wrong one changes another source's priority.
    pub const unsafe fn new(vector: u8, ipr: u8) -> Interrupt {
        Interrupt { vector, ipr }
    }

    /// The vector number (the n of rx-rt's `interrupt!(n, handler)`).
    pub const fn vector(self) -> u8 {
        self.vector
    }

    fn ier(self) -> (*mut u8, u8) {
        ((IER + (self.vector / 8) as usize) as _, 1 << (self.vector % 8))
    }

    fn ir(self) -> *mut u8 {
        (IR + self.vector as usize) as _
    }

    fn ipr(self) -> *mut u8 {
        (IPR + self.ipr as usize) as _
    }

    /// Sets IEN: the request is accepted once its priority and PSW allow.
    pub fn enable(self) {
        let (r, bit) = self.ier();
        // IERm holds eight sources: read-modify-write in a critical section.
        critical_section::with(|_| unsafe { write_volatile(r, read_volatile(r) | bit) });
    }

    /// Clears IEN. A request that comes in meanwhile stays in IR.
    pub fn disable(self) {
        let (r, bit) = self.ier();
        critical_section::with(|_| unsafe {
            write_volatile(r, read_volatile(r) & !bit);
            // Read back, so that the request is masked before this returns.
            let _ = read_volatile(r);
        });
    }

    /// Whether IEN is set.
    pub fn is_enabled(self) -> bool {
        let (r, bit) = self.ier();
        unsafe { read_volatile(r) & bit != 0 }
    }

    /// Sets the priority (IPR). Sources that share the IPR change too.
    pub fn set_priority(self, priority: Priority) {
        unsafe { write_volatile(self.ipr(), priority.0) }
    }

    /// Sets the priority to 0: the CPU never accepts the request. A source
    /// that only activates the DMAC ([`crate::dma`]) can stay there: the
    /// request reaches the DMAC whatever its priority.
    pub fn clear_priority(self) {
        unsafe { write_volatile(self.ipr(), 0) }
    }

    /// The priority, or `None` while it is 0 (never accepted).
    pub fn priority(self) -> Option<Priority> {
        Priority::new(unsafe { read_volatile(self.ipr()) } & 0x0f)
    }

    /// Whether the request flag IR is set.
    pub fn is_pending(self) -> bool {
        unsafe { read_volatile(self.ir()) & 1 != 0 }
    }

    /// Clears IR. For an edge-detected source (all the constants here) a
    /// request is then lost unless the peripheral makes a new one.
    pub fn clear_pending(self) {
        unsafe {
            write_volatile(self.ir(), 0);
            let _ = read_volatile(self.ir());
        }
    }
}

/// Requests SWINT (sets IR027), for example to run an executor at a low
/// priority from a higher one.
pub fn trigger_swint() {
    unsafe { write_volatile(SWINTR, 1) }
}

/// Requests SWINT2 (sets IR026).
pub fn trigger_swint2() {
    unsafe { write_volatile(SWINT2R, 1) }
}

/// The handler of group interrupt AL1 (vector 105, secure), which the
/// async drivers share: RSCI12's receive error (ERI12, `serial`) and
/// RI3C0's transfer end and events (TEI0 / EEI0, `i2c`). Each part looks
/// at its own GRPAL1_S bits. Register it with
/// [`bind_group_al1!`](crate::bind_group_al1).
#[cfg(feature = "async")]
pub extern "C" fn on_group_al1() {
    crate::serial::on_groupal1();
    crate::i2c::on_groupal1();
}

/// Registers [`on_group_al1`] (vector 105) with rx-rt's `interrupt!`: use
/// it once with the async serial port or I2C (feature `async`).
#[cfg(feature = "async")]
#[macro_export]
macro_rules! bind_group_al1 {
    () => {
        rx_rt::interrupt!(105, $crate::icu::on_group_al1);
    };
}
