//! I2C master in SCI6's simple I2C mode, on P00 = SSDA6 / P01 = SSCL6: the
//! AP-RX72M-0A's general-purpose EEPROM (24LC16B, with pull-ups on the
//! board, JSW6 ON as shipped), as a blocking embedded-hal `I2c` with 7-bit
//! addresses at about 100 kHz.
//!
//! It follows the manual's flows (42.7.4 to 42.7.6, SIMR2.IICINTM = 1)
//! by polling: SIMR3.IICSTIF for the start / restart / stop conditions, and
//! the ICU request flags of TXI6 (a frame done, IR087) and RXI6 (a byte
//! received, IR086) with the interrupts left disabled, as `serial` does.
//! SISR.IICACKR gives the ACK of each byte sent.
//!
//! The simple I2C mode is single master: there is no arbitration and no
//! bus-busy detection. A device stretching SCL is followed (SIMR2.IICCSC);
//! a wait that does not end in ICLK / 1000 polls (some 20 ms, as `i2c`'s
//! bus-busy wait: a poll reads a PCLKB register) is `Error::Timeout`, after
//! which the bus is released as in 42.7.7.

use core::ptr::{read_volatile, write_volatile};

use embedded_hal::i2c::{
    ErrorKind, ErrorType, I2c as I2cTrait, NoAcknowledgeSource, Operation, SevenBitAddress,
};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::i2c::{bytes_to_read, read_len, runs};
use crate::mstp::{self, Mstpcr};
use crate::pac;
use crate::timeout::OutOfRange;

/// PFS.PSEL of SSDA6 / SSCL6 (the SCI function, as TXD6 / RXD6).
const PSEL_SCI: u8 = 0b01010;

// ICU interrupt request flags of RXI6 / TXI6 (vectors 86 / 87).
const IR_RXI6: *mut u8 = 0x0008_7056 as _;
const IR_TXI6: *mut u8 = 0x0008_7057 as _;

// SCR: TIE, TE, RE (on); RIE added while receiving (the flows keep RXI off
// while sending)
const SCR_ON: u8 = 1 << 7 | 1 << 5 | 1 << 4;
const RIE: u8 = 1 << 6;
// SCMR: reserved bits 1 (F2h) with SDIR (MSB first), SINV = SMIF = 0
const SCMR_I2C: u8 = 0xf2 | 1 << 3;
// SEMR.NFEN; SNFR.NFCS = 001b (the noise filter on the clock / 1)
const NFEN: u8 = 1 << 5;
const NFCS_1: u8 = 0b001;
// SIMR1: IICM, and IICDL = 18h: an SSDA delay of 23-24 PCLK cycles (about
// 0.4 us at 60 MHz, over the 300 ns SCL fall time)
const SIMR1_I2C: u8 = 0x18 << 3 | 1;
// SIMR2
const IICINTM: u8 = 1 << 0;
const IICCSC: u8 = 1 << 1;
const IICACKT: u8 = 1 << 5;
// SIMR3: the request bits with IICSDAS = IICSCLS = 01b; IICSTIF; the pins
// released (both 11b); serial data / clock output (both 00b)
const STAREQ: u8 = 0x50 | 1 << 0;
const RSTAREQ: u8 = 0x50 | 1 << 1;
const STPREQ: u8 = 0x50 | 1 << 2;
const IICSTIF: u8 = 1 << 3;
const RELEASED: u8 = 0xf0;
const SERIAL: u8 = 0x00;
// SISR.IICACKR: the ACK bit received (1 = NACK)
const IICACKR: u8 = 1 << 0;

/// An I2C error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// No ACK for the address or a data byte (SISR.IICACKR).
    NoAcknowledge(NoAcknowledgeSource),
    /// A condition or a frame did not complete (SCL held low).
    Timeout,
}

impl embedded_hal::i2c::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Error::NoAcknowledge(s) => ErrorKind::NoAcknowledge(*s),
            Error::Timeout => ErrorKind::Other,
        }
    }
}

/// The bit rate setting: SMR.CKS = `cks`, BRR = `brr`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rate {
    pub cks: u8,
    pub brr: u8,
}

impl Rate {
    /// The fastest setting not over `hz`: B = pclk / (64 * 2^(2n - 1) *
    /// (N + 1)) (the simple I2C row of the BRR formulas), n = CKS. The SCL
    /// high / low widths are then about 47% / 53% of the period (table
    /// 42.26), so 100 kHz or less meets Standard mode.
    pub fn new(pclk: u32, hz: u32) -> Option<Self> {
        if hz == 0 {
            return None;
        }
        (0..4u8).find_map(|cks| {
            let div = 32u64 << (2 * cks);
            let n = (pclk as u64).div_ceil(div * hz as u64);
            (1..=256).contains(&n).then(|| Rate { cks, brr: (n - 1) as u8 })
        })
    }

    /// The resulting bit rate.
    pub fn hz(&self, pclk: u32) -> u32 {
        (pclk as u64 / ((32u64 << (2 * self.cks)) * (self.brr as u64 + 1))) as u32
    }
}

/// SCI6 as a simple I2C master.
pub struct SciI2c {
    sci: pac::Sci6,
    rate: Rate,
    /// The polls of a wait before `Error::Timeout`.
    polls: u32,
    sda: Pin<0x00, 0, Function>,
    scl: Pin<0x00, 1, Function>,
}

impl SciI2c {
    /// Sets up SCI6 (taking it out of module stop) as a simple I2C master
    /// at 100 kHz or just under (98.7 kHz at PCLKB 60 MHz) on P00 = SSDA6 /
    /// P01 = SSCL6, N-channel open drain (manual 42.7.4).
    ///
    /// Panics if PCLKB cannot give 100 kHz (see
    /// [`try_sci6`](Self::try_sci6)).
    pub fn sci6<DM, CM>(
        sci: pac::Sci6,
        sda: Pin<0x00, 0, DM>,
        scl: Pin<0x00, 1, CM>,
        clocks: &Clocks,
    ) -> Self {
        Self::try_sci6(sci, sda, scl, clocks).expect("PCLKB out of range")
    }

    /// [`sci6`](Self::sci6), or [`OutOfRange`] (the peripheral and pins
    /// untouched, and dropped) if PCLKB cannot give 100 kHz.
    pub fn try_sci6<DM, CM>(
        sci: pac::Sci6,
        sda: Pin<0x00, 0, DM>,
        scl: Pin<0x00, 1, CM>,
        clocks: &Clocks,
    ) -> Result<Self, OutOfRange> {
        let rate = Rate::new(clocks.pclkb, 100_000).ok_or(OutOfRange)?;
        mstp::start(Mstpcr::B, 25);
        sci.scr().write(|w| unsafe { w.bits(0) });
        let sda = sda.into_open_drain_function(PSEL_SCI);
        let scl = scl.into_open_drain_function(PSEL_SCI);
        sci.simr3().write(|w| unsafe { w.bits(RELEASED) });
        sci.smr().write(|w| unsafe { w.bits(rate.cks) });
        sci.scmr().write(|w| unsafe { w.bits(SCMR_I2C) });
        sci.brr().write(|w| unsafe { w.bits(rate.brr) });
        sci.semr().write(|w| unsafe { w.bits(NFEN) });
        sci.snfr().write(|w| unsafe { w.bits(NFCS_1) });
        sci.simr1().write(|w| unsafe { w.bits(SIMR1_I2C) });
        sci.simr2().write(|w| unsafe { w.bits(simr2(true)) });
        sci.spmr().write(|w| unsafe { w.bits(0) });
        sci.scr().write(|w| unsafe { w.bits(SCR_ON) });
        Ok(SciI2c { sci, rate, polls: clocks.iclk / 1000, sda, scl })
    }

    /// The bit rate setting in use.
    pub fn rate(&self) -> Rate {
        self.rate
    }

    /// Gives SCI6 and the pins back (SCI6 stopped; the pins still open
    /// drain until they change mode).
    pub fn free(self) -> (pac::Sci6, Pin<0x00, 0, Function>, Pin<0x00, 1, Function>) {
        self.sci.scr().write(|w| unsafe { w.bits(0) });
        (self.sci, self.sda, self.scl)
    }

    /// SIMR2.IICACKT: the ACK (`nack` = false) or NACK sent for a received
    /// byte; 1 also while receiving an ACK.
    fn set_nack(&self, nack: bool) {
        self.sci.simr2().write(|w| unsafe { w.bits(simr2(nack)) });
    }

    /// Generates a start, restart or stop condition and waits for it
    /// (IICSTIF), then sets the pins to serial output, or released after a
    /// stop.
    async fn condition(&mut self, w: &mut impl Waiter, req: u8) -> Result<(), Error> {
        self.sci.simr3().write(|w| unsafe { w.bits(req & !IICSTIF) });
        w.until(Event::Sti, self.polls).await?;
        let after = if req == STPREQ { RELEASED } else { SERIAL };
        self.sci.simr3().write(|w| unsafe { w.bits(after) });
        Ok(())
    }

    /// Sends a frame (TDR), waits for its end (TXI6) and returns whether it
    /// was ACKed.
    async fn send(&mut self, w: &mut impl Waiter, b: u8) -> Result<bool, Error> {
        w.arm(Event::Tx);
        self.sci.tdr().write(|w| unsafe { w.bits(b) });
        w.until(Event::Tx, self.polls).await?;
        Ok(self.sci.sisr().read().bits() & IICACKR == 0)
    }

    /// Receives a byte with the ACK or NACK set by `set_nack`: a dummy FFh
    /// to TDR, RXI6 for the byte, then TXI6 for the end of the frame.
    async fn receive(&mut self, w: &mut impl Waiter) -> Result<u8, Error> {
        w.arm(Event::Rx);
        w.arm(Event::Tx);
        self.sci.tdr().write(|w| unsafe { w.bits(0xff) });
        w.until(Event::Rx, self.polls).await?;
        let b = self.sci.rdr().read().bits();
        w.until(Event::Tx, self.polls).await?;
        Ok(b)
    }

    async fn run(&mut self, w: &mut impl Waiter, addr: u8, ops: &mut [Operation<'_>]) -> Result<(), Error> {
        let mut started = false;
        for (read, run, _) in runs(ops) {
            self.condition(w, if started { RSTAREQ } else { STAREQ }).await?;
            started = true;
            // The ACK of the address is received (IICACKT = 1).
            self.set_nack(true);
            if !self.send(w, addr << 1 | read as u8).await? {
                return Err(Error::NoAcknowledge(NoAcknowledgeSource::Address));
            }
            if read {
                // ACK for all but the last byte; a read of nothing reads one.
                let wanted = read_len(run);
                let total = wanted.max(1);
                let mut dropped = 0u8;
                let bytes = bytes_to_read(run).chain((wanted == 0).then_some(&mut dropped));
                self.sci.scr().write(|w| unsafe { w.bits(SCR_ON | RIE) });
                for (k, b) in bytes.enumerate() {
                    if k == 0 || k + 1 == total {
                        self.set_nack(k + 1 == total);
                    }
                    *b = self.receive(w).await?;
                }
                self.sci.scr().write(|w| unsafe { w.bits(SCR_ON) });
            } else {
                for op in run.iter() {
                    if let Operation::Write(bytes) = op {
                        for &b in bytes.iter() {
                            if !self.send(w, b).await? {
                                return Err(Error::NoAcknowledge(NoAcknowledgeSource::Data));
                            }
                        }
                    }
                }
            }
        }
        if started {
            self.condition(w, STPREQ).await?;
        }
        Ok(())
    }

    /// After an error: a stop condition; if that fails, the release of
    /// 42.7.7 (TE = RE = 0, SIMR3 = F0h, RDR read, TE = RE = 1).
    fn recover(&mut self) {
        self.sci.scr().write(|w| unsafe { w.bits(SCR_ON) });
        self.set_nack(true);
        if block_on(self.condition(&mut Spin, STPREQ)).is_err() {
            self.sci.scr().write(|w| unsafe { w.bits(0) });
            self.sci.simr3().write(|w| unsafe { w.bits(RELEASED) });
            let _ = self.sci.rdr().read().bits();
            self.sci.scr().write(|w| unsafe { w.bits(SCR_ON) });
        }
    }
}

/// What a transfer waits for: a received byte (RXI6), the end of a frame
/// (TXI6), the end of a condition (SIMR3.IICSTIF, STI6 = TEI6).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Event {
    Rx,
    Tx,
    Sti,
}

/// How a transfer waits: [`Spin`] polls the ICU request flags with the
/// interrupts disabled (the blocking `SciI2c`), `Irq` sleeps until the
/// interrupt (`AsyncSciI2c`). The transfer itself is the same code.
trait Waiter {
    /// Forgets an `event` that came before: the next `until` waits for a
    /// new one.
    fn arm(&mut self, event: Event);
    /// Waits for `event`; the polling waiter gives up after `polls`.
    async fn until(&mut self, event: Event, polls: u32) -> Result<(), Error>;
}

fn sti_done() -> bool {
    // SIMR3 of SCI6, read only (the transfer owns SCI6).
    let sci = unsafe { &*pac::Sci6::ptr() };
    sci.simr3().read().bits() & IICSTIF != 0
}

/// The polling waiter.
struct Spin;

impl Waiter for Spin {
    fn arm(&mut self, event: Event) {
        match event {
            Event::Rx => unsafe { write_volatile(IR_RXI6, 0) },
            Event::Tx => unsafe { write_volatile(IR_TXI6, 0) },
            Event::Sti => {}
        }
    }

    async fn until(&mut self, event: Event, polls: u32) -> Result<(), Error> {
        let done = || match event {
            Event::Rx => (unsafe { read_volatile(IR_RXI6) }) != 0,
            Event::Tx => (unsafe { read_volatile(IR_TXI6) }) != 0,
            Event::Sti => sti_done(),
        };
        (0..=polls).any(|_| done()).then_some(()).ok_or(Error::Timeout)
    }
}

/// Runs a future that never waits (one with [`Spin`]): it is ready at the
/// first poll.
fn block_on<F: core::future::Future>(f: F) -> F::Output {
    let mut f = core::pin::pin!(f);
    let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
    loop {
        if let core::task::Poll::Ready(r) = f.as_mut().poll(&mut cx) {
            return r;
        }
    }
}

/// SIMR2: IICINTM (TXI / RXI), IICCSC (clock synchronization), IICACKT.
fn simr2(nack: bool) -> u8 {
    IICINTM | IICCSC | if nack { IICACKT } else { 0 }
}

impl ErrorType for SciI2c {
    type Error = Error;
}

impl I2cTrait<SevenBitAddress> for SciI2c {
    /// A read of no bytes reads one byte (with the NACK) and drops it: the
    /// NACK of the last byte is what ends a read in this mode.
    fn transaction(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        let r = block_on(self.run(&mut Spin, address, operations));
        if r.is_err() {
            self.recover();
        }
        r
    }
}

#[cfg(feature = "async")]
pub use asynch::{AsyncSciI2c, on_groupbl0, on_rxi6, on_txi6};

/// SCI6's simple I2C as an interrupt-driven embedded-hal-async `I2c`
/// (feature `async`): the same transfer as the blocking driver, sleeping
/// where that one polls.
///
/// RXI6 / TXI6 (vectors 86 / 87) are enabled: their handlers note the
/// event (accepting the request clears its IR, which the blocking driver
/// polls) and wake the transfer. The end of a condition (IICSTIF) comes as
/// STI6 = TEI6, in group interrupt BL0 (vector 110, GRPBL0.IS12), while a
/// wait has SCR.TEIE set; its handler clears TEIE and wakes.
///
/// There is no bound on a wait (the blocking driver's polls): a device
/// that holds SCL stops the transfer until a timeout around it (embassy-
/// time's `with_timeout`) drops it. Dropping a transaction's future ends it
/// as an error does: a stop condition, or the release of 42.7.7.
///
/// Register the handlers with [`bind_sci_i2c_interrupts!`](crate::bind_sci_i2c_interrupts)
/// and [`bind_group_bl0!`](crate::bind_group_bl0) (shared with the async
/// serial port).
#[cfg(feature = "async")]
mod asynch {
    use core::cell::RefCell;
    use core::future::poll_fn;
    use core::ptr::{read_volatile, write_volatile};
    use core::sync::atomic::{AtomicBool, Ordering::Relaxed};
    use core::task::{Poll, Waker};

    use critical_section::Mutex;
    use embedded_hal::i2c::{ErrorType, Operation, SevenBitAddress};

    use super::{Error, Event, SciI2c, Waiter, sti_done};
    use crate::icu::{Interrupt, Priority};

    const RXI6: Interrupt = unsafe { Interrupt::new(86, 86) };
    const TXI6: Interrupt = unsafe { Interrupt::new(87, 87) };
    /// Group BL0: TEI6 is bit 12.
    const GROUP_BL0: Interrupt = unsafe { Interrupt::new(110, 110) };
    const GRPBL0: *const u32 = 0x0008_7630 as _;
    const GENBL0: *mut u32 = 0x0008_7670 as _;
    const TEI6: u32 = 1 << 12;
    /// SCR of SCI6 (SCI6 + 02h), and its TEIE (the STI6 request in simple
    /// I2C mode).
    const SCR: *mut u8 = 0x0008_a0c2 as _;
    const TEIE: u8 = 1 << 2;

    static RX: AtomicBool = AtomicBool::new(false);
    static TX: AtomicBool = AtomicBool::new(false);
    static WAKER: Mutex<RefCell<Option<Waker>>> = Mutex::new(RefCell::new(None));

    fn wake() {
        critical_section::with(|cs| {
            if let Some(w) = WAKER.borrow_ref_mut(cs).take() {
                w.wake();
            }
        });
    }

    fn set_teie(on: bool) {
        critical_section::with(|_| unsafe {
            let scr = read_volatile(SCR);
            write_volatile(SCR, if on { scr | TEIE } else { scr & !TEIE });
        });
    }

    /// The handler of RXI6 (vector 86).
    pub extern "C" fn on_rxi6() {
        RX.store(true, Relaxed);
        wake();
    }

    /// The handler of TXI6 (vector 87).
    pub extern "C" fn on_txi6() {
        TX.store(true, Relaxed);
        wake();
    }

    /// This driver's part of the group BL0 handler
    /// ([`icu::on_group_bl0`](crate::icu::on_group_bl0)).
    pub fn on_groupbl0() {
        if unsafe { read_volatile(GRPBL0) } & TEI6 != 0 {
            set_teie(false);
            wake();
        }
    }

    fn done(event: Event) -> bool {
        match event {
            Event::Rx => RX.load(Relaxed),
            Event::Tx => TX.load(Relaxed),
            Event::Sti => sti_done(),
        }
    }

    /// The interrupt waiter.
    struct Irq;

    impl Waiter for Irq {
        fn arm(&mut self, event: Event) {
            match event {
                Event::Rx => RX.store(false, Relaxed),
                Event::Tx => TX.store(false, Relaxed),
                Event::Sti => {}
            }
        }

        async fn until(&mut self, event: Event, _polls: u32) -> Result<(), Error> {
            poll_fn(|cx| {
                if done(event) {
                    return Poll::Ready(Ok(()));
                }
                critical_section::with(|cs| {
                    *WAKER.borrow_ref_mut(cs) = Some(cx.waker().clone());
                });
                if event == Event::Sti {
                    set_teie(true);
                }
                // It may have come before the waker was set.
                if done(event) {
                    if event == Event::Sti {
                        set_teie(false);
                    }
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                }
            })
            .await
        }
    }

    /// The interrupt-driven simple I2C; see the [module](self).
    pub struct AsyncSciI2c {
        i2c: SciI2c,
    }

    impl SciI2c {
        /// Enables RXI6 / TXI6 and group BL0 with TEI6 at `priority`
        /// (group BL0's priority is shared with the async serial port's
        /// ERI2), and returns the interrupt-driven I2C.
        pub fn into_async(self, priority: Priority) -> AsyncSciI2c {
            set_teie(false);
            critical_section::with(|_| unsafe {
                write_volatile(GENBL0, read_volatile(GENBL0) | TEI6);
            });
            for i in [RXI6, TXI6] {
                i.clear_pending();
            }
            for i in [RXI6, TXI6, GROUP_BL0] {
                i.set_priority(priority);
                i.enable();
            }
            AsyncSciI2c { i2c: self }
        }
    }

    impl AsyncSciI2c {
        /// Disables its interrupts and gives back the blocking I2C.
        pub fn free(self) -> SciI2c {
            set_teie(false);
            RXI6.disable();
            TXI6.disable();
            critical_section::with(|_| unsafe {
                write_volatile(GENBL0, read_volatile(GENBL0) & !TEI6);
            });
            self.i2c
        }

        /// The bit rate setting in use.
        pub fn rate(&self) -> super::Rate {
            self.i2c.rate()
        }
    }

    /// Ends a transaction cut short (its future dropped) as an error does.
    struct Recover<'a>(&'a mut SciI2c, bool);

    impl Drop for Recover<'_> {
        fn drop(&mut self) {
            if !self.1 {
                set_teie(false);
                self.0.recover();
            }
        }
    }

    impl ErrorType for AsyncSciI2c {
        type Error = Error;
    }

    impl embedded_hal_async::i2c::I2c<SevenBitAddress> for AsyncSciI2c {
        /// A read of no bytes reads one byte (with the NACK) and drops it,
        /// as the blocking driver does.
        async fn transaction(
            &mut self,
            address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Error> {
            let mut g = Recover(&mut self.i2c, false);
            let r = g.0.run(&mut Irq, address, operations).await;
            set_teie(false);
            if r.is_err() {
                g.0.recover();
            }
            g.1 = true;
            r
        }
    }
}

/// Registers the async simple I2C's RXI6 / TXI6 handlers (vectors 86 / 87)
/// with rx-rt's `interrupt!`; its condition ends come through group BL0:
/// use [`bind_group_bl0!`](crate::bind_group_bl0) too. Feature `async`.
#[cfg(feature = "async")]
#[macro_export]
macro_rules! bind_sci_i2c_interrupts {
    () => {
        rx_rt::interrupt!(86, $crate::sci_i2c::on_rxi6);
        rx_rt::interrupt!(87, $crate::sci_i2c::on_txi6);
    };
}
