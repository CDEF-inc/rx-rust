//! Asynchronous serial (UART) on SCI2, through the AP-RX72M-0A's serial
//! connector CN8 (P13 = TXD2, P12 = RXD2), as blocking `embedded-io`
//! `Read` / `Write`. 8 data bits, no parity, 1 stop bit.
//!
//! The baud rate comes from SMR.CKS, SEMR.ABCS / BGDM and BRR: the
//! combination closest to the requested rate (PCLKB 60 MHz, 115200 bps:
//! ABCS = BGDM = 1, BRR = 64, 115385 bps, +0.16%).
//!
//! Like Renesas' FIT driver, which writes SSR bits 7 and 6 as 1 and does
//! not use them, the driver does not poll TDRE / RDRF but the interrupt
//! requests: with SCR.TIE / RIE set and the interrupts left disabled in the
//! ICU, IR063 (TXI2) is set when TDR can take a byte and IR062 (RXI2) when
//! RDR holds one. SSR gives TEND and the errors.
//!
//! RXD2 gets the internal pull-up, so that an unconnected CN8 idles high
//! instead of floating into framing errors.
//!
//! Checked on the board with a USB serial adapter on CN8
//! (examples/serial-echo, scripts/hw/serial-echo.ps1): sending, receiving
//! (RXI2 as IR062, `Read`, `ReadReady`), 4096 bytes back to back at 115200
//! bps without an overrun, and a break as `Error::Framing`. See
//! docs/NOTES-hal.md.

use core::fmt;
use core::ptr::{read_volatile, write_volatile};

use embedded_io::{ErrorKind, ErrorType, Read, ReadReady, Write, WriteReady};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::mstp::{self, Mstpcr};
use crate::timeout::{self, OutOfRange};
use crate::pac;

/// PFS.PSEL of the SCI functions (TXDn / RXDn).
const PSEL_SCI: u8 = 0b01010;

// ICU interrupt request flags of RXI2 / TXI2 (vectors 62 / 63).
const IR_RXI2: *mut u8 = 0x0008_703e as _;
const IR_TXI2: *mut u8 = 0x0008_703f as _;

// SSR bits (7 and 6 are written as 1)
const SSR_KEEP: u8 = 0xc0;
const ORER: u8 = 1 << 5;
const FER: u8 = 1 << 4;
const PER: u8 = 1 << 3;
const TEND: u8 = 1 << 2;

/// A receive error (the received data with the error is dropped), or a
/// transmission that did not finish in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    Overrun,
    Framing,
    Parity,
    /// TDR did not empty (TXI2) or the last byte did not leave (TEND)
    /// within four character times.
    Timeout,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Error::Overrun => "overrun",
            Error::Framing => "framing error",
            Error::Parity => "parity error",
            Error::Timeout => "transmit timeout",
        })
    }
}

impl core::error::Error for Error {}

impl embedded_io::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Error::Overrun => ErrorKind::Other,
            Error::Framing | Error::Parity => ErrorKind::InvalidData,
            Error::Timeout => ErrorKind::TimedOut,
        }
    }
}

/// The baud rate settings: SMR.CKS, SEMR.ABCS, SEMR.BGDM, BRR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaudSettings {
    pub cks: u8,
    pub abcs: bool,
    pub bgdm: bool,
    pub brr: u8,
    /// The resulting rate in bits per second.
    pub baud: u32,
}

impl BaudSettings {
    /// The settings closest to `baud` for the peripheral clock `pclk`:
    /// bit rate = pclk / (k * 4^cks * (brr + 1)), k = 32 (ABCS = BGDM = 0),
    /// 16 (one of them) or 8 (both). For each k and CKS both integer
    /// divisors next to the ideal one are tried (the rate is inversely
    /// proportional to the divisor, so rounding it does not give the
    /// smallest error). Among equal errors the larger k (more samples per
    /// bit) wins.
    pub fn new(pclk: u32, baud: u32) -> Option<Self> {
        if baud == 0 {
            return None;
        }
        let mut best: Option<(u64, BaudSettings)> = None;
        for (k, abcs, bgdm) in [(32u64, false, false), (16, true, false), (8, true, true)] {
            for cks in 0..4u8 {
                let div = k << (2 * cks);
                let lo = pclk as u64 / (div * baud as u64);
                for n in [lo, lo + 1] {
                    if !(1..=256).contains(&n) {
                        continue;
                    }
                    let actual = (pclk as u64 / (div * n)) as u32;
                    let err = (actual as i64 - baud as i64).unsigned_abs();
                    if best.is_none_or(|(e, _)| err < e) {
                        let brr = (n - 1) as u8;
                        best = Some((err, BaudSettings { cks, abcs, bgdm, brr, baud: actual }));
                    }
                }
            }
        }
        best.map(|(_, s)| s)
    }
}

/// SCI2 as a UART.
pub struct Serial {
    sci: pac::Sci2,
    settings: BaudSettings,
    /// A receive error met after the first byte of a `read`, returned by
    /// the next `read`.
    pending: Option<Error>,
    /// The bound of a transmit wait: four character times.
    tx_spins: u32,
    _tx: Pin<0x01, 3, Function>,
    _rx: Pin<0x01, 2, Function>,
}

impl Serial {
    /// Sets up SCI2 (taking it out of module stop) at `baud` bit/s on
    /// P13 / P12, receiving and transmitting.
    ///
    /// Panics if no setting is within 3% of `baud` (see
    /// [`try_sci2`](Self::try_sci2)).
    pub fn sci2<TM, RM>(
        sci: pac::Sci2,
        tx: Pin<0x01, 3, TM>,
        rx: Pin<0x01, 2, RM>,
        baud: u32,
        clocks: &Clocks,
    ) -> Self {
        Self::try_sci2(sci, tx, rx, baud, clocks).expect("baud rate out of range")
    }

    /// [`sci2`](Self::sci2), or [`OutOfRange`] (the peripheral and pins
    /// untouched, and dropped) if no setting is within 3% of `baud`.
    pub fn try_sci2<TM, RM>(
        sci: pac::Sci2,
        tx: Pin<0x01, 3, TM>,
        rx: Pin<0x01, 2, RM>,
        baud: u32,
        clocks: &Clocks,
    ) -> Result<Self, OutOfRange> {
        let s = BaudSettings::new(clocks.pclkb, baud).ok_or(OutOfRange)?;
        if (s.baud as u64).abs_diff(baud as u64) * 100 > 3 * baud as u64 {
            return Err(OutOfRange);
        }
        let tx_spins = timeout::spins(clocks.iclk, 4 * 10_000_000 / s.baud + 1);
        mstp::start(Mstpcr::B, 29);
        sci.scr().write(|w| unsafe { w.bits(0) });
        let tx = tx.into_function(PSEL_SCI);
        // PCR stays set in the function mode.
        let rx = rx.into_pull_up_input().into_function(PSEL_SCI);
        sci.simr1().write(|w| unsafe { w.bits(0) });
        sci.spmr().write(|w| unsafe { w.bits(0) });
        // Asynchronous, 8 bits, no parity, 1 stop bit (SCMR keeps its reset
        // value: serial communication interface mode, LSB first).
        sci.smr().write(|w| unsafe { w.cks().bits(s.cks) });
        sci.semr().write(|w| w.abcs().bit(s.abcs).bgdm().bit(s.bgdm));
        sci.brr().write(|w| unsafe { w.bits(s.brr) });
        // Wait at least one bit time before enabling (as the manual asks):
        // a loop of at least one ICLK cycle per iteration, kept by a
        // volatile read (core::hint::spin_loop is nothing on RX, and a loop
        // of only that is removed by the optimizer).
        let keep = 0u8;
        for _ in 0..clocks.iclk / s.baud {
            unsafe { core::ptr::read_volatile(&keep) };
        }
        // TE with TIE in the same write raises TXI (TDR empty).
        unsafe {
            write_volatile(IR_RXI2, 0);
            write_volatile(IR_TXI2, 0);
        }
        let v = sci.ssr().read().bits();
        sci.ssr().write(|w| unsafe { w.bits((v & !(ORER | FER | PER)) | SSR_KEEP) });
        sci.scr()
            .write(|w| w.te().set_bit().re().set_bit().tie().set_bit().rie().set_bit());
        Ok(Serial { sci, settings: s, pending: None, tx_spins, _tx: tx, _rx: rx })
    }

    /// Whether TDR can take a byte (TXI2 requested).
    pub fn tx_ready(&self) -> bool {
        unsafe { read_volatile(IR_TXI2) != 0 }
    }

    /// Whether RDR holds a received byte (RXI2 requested).
    pub fn rx_ready(&self) -> bool {
        unsafe { read_volatile(IR_RXI2) != 0 }
    }

    /// The baud rate settings in use.
    pub fn settings(&self) -> BaudSettings {
        self.settings
    }

    fn ssr(&self) -> u8 {
        self.sci.ssr().read().bits()
    }

    /// Clears the given SSR flags (written as 0; bits 7 and 6 as 1, as
    /// Renesas' FIT driver does).
    fn clear(&self, flags: u8) {
        let v = self.ssr();
        self.sci.ssr().write(|w| unsafe { w.bits((v & !flags) | SSR_KEEP) });
    }

    /// Returns the next byte if one has arrived, or a receive error.
    fn try_read_byte(&mut self) -> Option<Result<u8, Error>> {
        let ssr = self.ssr();
        if ssr & (ORER | FER | PER) != 0 {
            let _ = self.sci.rdr().read().bits();
            self.clear(ORER | FER | PER);
            unsafe { write_volatile(IR_RXI2, 0) };
            return Some(Err(if ssr & ORER != 0 {
                Error::Overrun
            } else if ssr & FER != 0 {
                Error::Framing
            } else {
                Error::Parity
            }));
        }
        if !self.rx_ready() {
            return None;
        }
        unsafe { write_volatile(IR_RXI2, 0) };
        Some(Ok(self.sci.rdr().read().bits()))
    }
}

impl ErrorType for Serial {
    type Error = Error;
}

impl Write for Serial {
    /// Writes the whole buffer, waiting for TXI2 before each byte;
    /// [`Error::Timeout`] if it does not come within four character times.
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        for &b in buf {
            timeout::wait(self.tx_spins, || self.tx_ready()).map_err(|_| Error::Timeout)?;
            unsafe { write_volatile(IR_TXI2, 0) };
            self.sci.tdr().write(|w| unsafe { w.bits(b) });
        }
        Ok(buf.len())
    }

    /// Waits until the last byte has left the shift register (TEND);
    /// [`Error::Timeout`] after four character times.
    fn flush(&mut self) -> Result<(), Error> {
        timeout::wait(self.tx_spins, || self.ssr() & TEND != 0).map_err(|_| Error::Timeout)
    }
}

impl WriteReady for Serial {
    fn write_ready(&mut self) -> Result<bool, Error> {
        Ok(self.tx_ready())
    }
}

impl Read for Serial {
    /// Waits for one byte, then takes the ones already received. A receive
    /// error after the first byte ends the read with the bytes so far and
    /// is returned by the next `read`.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        if let Some(e) = self.pending.take() {
            return Err(e);
        }
        if buf.is_empty() {
            return Ok(0);
        }
        let first = loop {
            if let Some(r) = self.try_read_byte() {
                break r?;
            }
        };
        buf[0] = first;
        let mut n = 1;
        while n < buf.len() {
            match self.try_read_byte() {
                Some(Ok(b)) => {
                    buf[n] = b;
                    n += 1;
                }
                Some(Err(e)) => {
                    self.pending = Some(e);
                    break;
                }
                None => break,
            }
        }
        Ok(n)
    }
}

impl ReadReady for Serial {
    fn read_ready(&mut self) -> Result<bool, Error> {
        Ok(self.pending.is_some() || self.rx_ready() || self.ssr() & (ORER | FER | PER) != 0)
    }
}

#[cfg(feature = "async")]
pub use asynch::{AsyncSerial, on_groupbl0, on_rxi2, on_txi2};

/// The interrupt-driven SCI2 (feature `async`): `embedded-io-async`
/// `Read` / `Write`, with the ICU taking the requests the blocking driver
/// polls.
///
/// - **Receiving:** RXI2 (vector 62) puts each byte into a 128-byte ring
///   buffer and wakes the reader. A receive error comes as ERI2, in group
///   interrupt BL0 (vector 110, GRPBL0.IS5): the handler clears the error
///   and keeps it, with its place in the stream: `read` returns the bytes
///   before it, then the error, then the bytes after it. A full buffer
///   drops the byte and counts as an overrun.
/// - **Transmitting:** SCR.TIE stays set, so TXI2 (vector 63) comes once
///   each time TDR empties; its handler marks TDR free and wakes the
///   writer, which writes the next byte.
///
/// The three vectors belong to this driver: register its handlers with
/// [`bind_serial_interrupts!`](crate::bind_serial_interrupts), and leave
/// group BL0's other sources off.
#[cfg(feature = "async")]
mod asynch {
    use core::cell::{Cell, RefCell};
    use core::future::poll_fn;
    use core::ptr::{read_volatile, write_volatile};
    use core::task::{Poll, Waker};

    use critical_section::Mutex;
    use embedded_io::ErrorType;

    use super::{Error, FER, ORER, PER, SSR_KEEP, Serial, TEND};
    use crate::icu::{Interrupt, Priority};
    use crate::pac;

    /// Group interrupt BL0, which has ERI2.
    const GROUPBL0_VECTOR: u8 = 110;
    /// GRPBL0 (request status) / GENBL0 (enable): ERI2 is bit 5.
    const GRPBL0: *const u32 = 0x0008_7630 as _;
    const GENBL0: *mut u32 = 0x0008_7670 as _;
    const ERI2: u32 = 1 << 5;

    const RX_BUF: usize = 128;

    struct Rx {
        buf: [u8; RX_BUF],
        head: usize,
        len: usize,
        /// The first receive error not returned yet, and how many of the
        /// buffered bytes came before it. Later errors are not kept: the
        /// stream is already known to be damaged there.
        error: Option<(Error, usize)>,
        waker: Option<Waker>,
    }

    impl Rx {
        /// Keeps `e` after the bytes buffered so far.
        fn set_error(&mut self, e: Error) {
            if self.error.is_none() {
                self.error = Some((e, self.len));
            }
        }
    }

    static RX: Mutex<RefCell<Rx>> = Mutex::new(RefCell::new(Rx {
        buf: [0; RX_BUF],
        head: 0,
        len: 0,
        error: None,
        waker: None,
    }));
    /// Whether TDR can take a byte (a TXI2 not used yet).
    static TX_FREE: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
    static TX_WAKER: Mutex<RefCell<Option<Waker>>> = Mutex::new(RefCell::new(None));

    fn sci() -> pac::Sci2 {
        // The handlers and AsyncSerial share SCI2: the handlers read RDR
        // and clear SSR's errors, AsyncSerial writes TDR.
        unsafe { pac::Sci2::steal() }
    }

    fn wake(w: &mut Option<Waker>) {
        if let Some(w) = w.take() {
            w.wake();
        }
    }

    fn groupbl0() -> Interrupt {
        unsafe { Interrupt::new(GROUPBL0_VECTOR, GROUPBL0_VECTOR) }
    }

    /// The interrupt-driven SCI2; see the [module](self).
    pub struct AsyncSerial {
        serial: Serial,
    }

    impl Serial {
        /// Enables RXI2 / TXI2 (vectors 62 / 63) and ERI2 (group BL0,
        /// vector 110) at `priority` and returns the interrupt-driven
        /// serial port. Register the handlers with
        /// [`bind_serial_interrupts!`](crate::bind_serial_interrupts).
        pub fn into_async(self, priority: Priority) -> AsyncSerial {
            critical_section::with(|cs| {
                let mut rx = RX.borrow_ref_mut(cs);
                rx.head = 0;
                rx.len = 0;
                rx.error = self.pending.map(|e| (e, 0));
                rx.waker = None;
                // IR062 / IR063 are left as the blocking driver left them: a
                // byte in RDR or an empty TDR is taken by the handler as
                // soon as it is enabled.
                TX_FREE.borrow(cs).set(false);
                unsafe { write_volatile(GENBL0, read_volatile(GENBL0) | ERI2) };
            });
            let group = groupbl0();
            group.clear_pending();
            for irq in [Interrupt::RXI2, Interrupt::TXI2, group] {
                irq.set_priority(priority);
                irq.enable();
            }
            AsyncSerial { serial: self }
        }
    }

    impl AsyncSerial {
        /// The baud rate settings in use.
        pub fn settings(&self) -> super::BaudSettings {
            self.serial.settings()
        }
    }

    impl ErrorType for AsyncSerial {
        type Error = Error;
    }

    impl embedded_io_async::Read for AsyncSerial {
        /// Waits until a byte has arrived, then takes the buffered ones (up
        /// to `buf.len()`), stopping at a receive error: the error is
        /// returned by the next `read`, before the bytes after it.
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
            if buf.is_empty() {
                return Ok(0);
            }
            poll_fn(|cx| {
                critical_section::with(|cs| {
                    let mut rx = RX.borrow_ref_mut(cs);
                    if let Some((e, 0)) = rx.error {
                        rx.error = None;
                        Poll::Ready(Err(e))
                    } else if rx.len > 0 {
                        // Up to the error, if there is one.
                        let before = rx.error.map_or(rx.len, |(_, at)| at);
                        let n = before.min(buf.len());
                        for b in buf.iter_mut().take(n) {
                            *b = rx.buf[rx.head];
                            rx.head = (rx.head + 1) % RX_BUF;
                        }
                        rx.len -= n;
                        if let Some((_, at)) = &mut rx.error {
                            *at -= n;
                        }
                        Poll::Ready(Ok(n))
                    } else {
                        rx.waker = Some(cx.waker().clone());
                        Poll::Pending
                    }
                })
            })
            .await
        }
    }

    impl embedded_io_async::Write for AsyncSerial {
        /// Writes the whole buffer, sleeping until TXI2 says TDR is free.
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
            for &b in buf {
                poll_fn(|cx| {
                    critical_section::with(|cs| {
                        let free = TX_FREE.borrow(cs);
                        if free.get() {
                            free.set(false);
                            Poll::Ready(())
                        } else {
                            *TX_WAKER.borrow_ref_mut(cs) = Some(cx.waker().clone());
                            Poll::Pending
                        }
                    })
                })
                .await;
                self.serial.sci.tdr().write(|w| unsafe { w.bits(b) });
            }
            Ok(buf.len())
        }

        /// Waits until the last byte has left the shift register (TEND),
        /// yielding meanwhile (at most a character time).
        async fn flush(&mut self) -> Result<(), Error> {
            poll_fn(|cx| {
                if self.serial.ssr() & TEND != 0 {
                    Poll::Ready(Ok(()))
                } else {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            })
            .await
        }
    }

    /// The RXI2 handler (vector 62): buffers the received byte.
    pub extern "C" fn on_rxi2() {
        let b = sci().rdr().read().bits();
        critical_section::with(|cs| {
            let mut rx = RX.borrow_ref_mut(cs);
            if rx.len < RX_BUF {
                let at = (rx.head + rx.len) % RX_BUF;
                rx.buf[at] = b;
                rx.len += 1;
            } else {
                rx.set_error(Error::Overrun);
            }
            wake(&mut rx.waker);
        })
    }

    /// The TXI2 handler (vector 63): TDR is empty; wakes the writer.
    pub extern "C" fn on_txi2() {
        critical_section::with(|cs| {
            TX_FREE.borrow(cs).set(true);
            wake(&mut TX_WAKER.borrow_ref_mut(cs));
        })
    }

    /// The group BL0 handler (vector 110): a receive error (ERI2) is
    /// cleared and kept for the next `read`.
    pub extern "C" fn on_groupbl0() {
        if unsafe { read_volatile(GRPBL0) } & ERI2 == 0 {
            return;
        }
        let s = sci();
        let ssr = s.ssr().read().bits();
        let e = if ssr & ORER != 0 {
            Error::Overrun
        } else if ssr & FER != 0 {
            Error::Framing
        } else {
            Error::Parity
        };
        let _ = s.rdr().read().bits();
        s.ssr().write(|w| unsafe { w.bits((ssr & !(ORER | FER | PER)) | SSR_KEEP) });
        critical_section::with(|cs| {
            let mut rx = RX.borrow_ref_mut(cs);
            rx.set_error(e);
            wake(&mut rx.waker);
        })
    }
}

/// Registers the interrupt-driven SCI2's handlers (feature `async`) with
/// rx-rt's `interrupt!`: RXI2 (62), TXI2 (63), group BL0 (110). Use it
/// once, in a crate that depends on rx-rt.
#[cfg(feature = "async")]
#[macro_export]
macro_rules! bind_serial_interrupts {
    () => {
        rx_rt::interrupt!(62, $crate::serial::on_rxi2);
        rx_rt::interrupt!(63, $crate::serial::on_txi2);
        rx_rt::interrupt!(110, $crate::serial::on_groupbl0);
    };
}
