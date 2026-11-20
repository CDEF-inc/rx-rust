//! Asynchronous serial (UART) on RSCI12, through the EK-RX74N's USB serial
//! converter on J27 (P114 = TXD012-B, P115 = RXD012-B), as blocking
//! `embedded-io` `Read` / `Write`. 8 data bits, no parity, 1 stop bit, no
//! FIFO.
//!
//! The baud rate comes from SCR2's CKS, ABCS, BGDM, BRR and, with the bit
//! rate modulation, MDDR: the combination closest to the requested rate
//! (SCICLK 8 MHz, 115200 bps: ABCS = BGDM = 1, BRR = 7, MDDR = 236,
//! 115234 bps, +0.03%).

use core::fmt;

use embedded_io::{ErrorKind, ErrorType, Read, ReadReady, Write, WriteReady};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::mstp::{self, Mstpcr};
use crate::pac;

/// PSEL of TXD012-B / RXD012-B.
const PSEL_RSCI: u8 = 0b000_1010;

/// A receive error. The received data with the error is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Overrun,
    Framing,
    Parity,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Error::Overrun => "overrun",
            Error::Framing => "framing error",
            Error::Parity => "parity error",
        })
    }
}

impl core::error::Error for Error {}

impl embedded_io::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Error::Overrun => ErrorKind::Other,
            Error::Framing | Error::Parity => ErrorKind::InvalidData,
        }
    }
}

/// The baud rate settings of SCR2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaudSettings {
    pub cks: u8,
    pub abcs: bool,
    pub bgdm: bool,
    pub brr: u8,
    /// MDDR, with the bit rate modulation on (`None`: off).
    pub mddr: Option<u8>,
    /// The resulting rate in bits per second.
    pub baud: u32,
}

impl BaudSettings {
    /// The settings closest to `baud` for the operating clock `tclk`:
    /// bit rate = tclk * m / 256 / (k * 4^cks * (brr + 1)), k = 32 (ABCS =
    /// BGDM = 0), 16 (one of them) or 8 (both), m = MDDR (128-255) or 256
    /// without the modulation. Among equal errors the larger k (more samples
    /// per bit) and no modulation win.
    pub fn new(tclk: u32, baud: u32) -> Option<Self> {
        if baud == 0 {
            return None;
        }
        let mut best: Option<(u64, BaudSettings)> = None;
        for (k, abcs, bgdm) in [(32u64, false, false), (16, true, false), (8, true, true)] {
            for cks in 0..4u8 {
                let div = k << (2 * cks);
                for n in 1..=256u64 {
                    // m for this divisor, then its neighbour; 256 is no
                    // modulation.
                    let m0 = 256 * baud as u64 * div * n / tclk as u64;
                    for m in [m0, m0 + 1] {
                        if !(128..=256).contains(&m) {
                            continue;
                        }
                        let actual = (tclk as u64 * m / 256 / (div * n)) as u32;
                        let err = (actual as i64 - baud as i64).unsigned_abs();
                        if best.is_none_or(|(e, _)| err < e) {
                            let mddr = (m < 256).then_some(m as u8);
                            let brr = (n - 1) as u8;
                            let s = BaudSettings { cks, abcs, bgdm, brr, mddr, baud: actual };
                            best = Some((err, s));
                        }
                    }
                }
            }
        }
        best.map(|(_, s)| s)
    }
}

/// RSCI12 as a UART.
pub struct Serial {
    rsci: pac::Rsci12,
    settings: BaudSettings,
    /// A receive error met after the first byte of a `read`, returned by
    /// the next `read`.
    pending: Option<Error>,
    _tx: Pin<0x01, 14, Function>,
    _rx: Pin<0x01, 15, Function>,
}

impl Serial {
    /// Sets up RSCI12 (taking it out of module stop) at `baud` bit/s on
    /// P114 / P115, receiving and transmitting.
    ///
    /// Panics if no setting is within 3% of `baud`.
    pub fn rsci12<TM, RM>(
        rsci: pac::Rsci12,
        tx: Pin<0x01, 14, TM>,
        rx: Pin<0x01, 15, RM>,
        baud: u32,
        clocks: &Clocks,
    ) -> Self {
        let s = BaudSettings::new(clocks.sciclk, baud).expect("baud rate out of range");
        assert!((s.baud as u64).abs_diff(baud as u64) * 100 <= 3 * baud as u64);
        mstp::start(Mstpcr::B, 0);
        rsci.scr0().write(|w| unsafe { w.bits(0) });
        // Asynchronous, 8 bits, 1 stop bit, LSB first, no FIFO. SCR3 keeps
        // its reset value, which is that. SCR1: no parity.
        rsci.scr2().modify(|_, w| unsafe {
            w.cks().bits(s.cks).abcs().bit(s.abcs).bgdm().bit(s.bgdm).brr().bits(s.brr);
            w.brme().bit(s.mddr.is_some()).mddr().bits(s.mddr.unwrap_or(0xff))
        });
        rsci.scr1().write(|w| unsafe { w.bits(0) });
        let tx = tx.into_function(PSEL_RSCI);
        // PCR stays set in the function mode.
        let rx = rx.into_pull_up_input().into_function(PSEL_RSCI);
        rsci.sscr().write(|w| {
            w.rdrfc().set_bit().aferc().set_bit().aperc().set_bit().orerc().set_bit()
        });
        rsci.scr0().write(|w| w.te().set_bit().re().set_bit());
        Serial { rsci, settings: s, pending: None, _tx: tx, _rx: rx }
    }

    /// The baud rate settings in use.
    pub fn settings(&self) -> BaudSettings {
        self.settings
    }

    /// Returns the next byte if one has arrived, or a receive error.
    fn try_read_byte(&mut self) -> Option<Result<u8, Error>> {
        let ssr = self.rsci.ssr().read();
        if ssr.orer().bit_is_set() || ssr.afer().bit_is_set() || ssr.aper().bit_is_set() {
            let _ = self.rsci.rdr().read().bits();
            self.rsci.sscr().write(|w| {
                w.rdrfc().set_bit().aferc().set_bit().aperc().set_bit().orerc().set_bit()
            });
            return Some(Err(if ssr.orer().bit_is_set() {
                Error::Overrun
            } else if ssr.afer().bit_is_set() {
                Error::Framing
            } else {
                Error::Parity
            }));
        }
        if ssr.rdrf().bit_is_clear() {
            return None;
        }
        Some(Ok(self.rsci.rdr().read().rdat().bits() as u8))
    }

    fn rx_pending(&self) -> bool {
        let ssr = self.rsci.ssr().read();
        ssr.rdrf().bit_is_set()
            || ssr.orer().bit_is_set()
            || ssr.afer().bit_is_set()
            || ssr.aper().bit_is_set()
    }
}

impl ErrorType for Serial {
    type Error = Error;
}

impl Write for Serial {
    /// Writes the whole buffer, waiting for TDRE before each byte.
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        for &b in buf {
            while self.rsci.ssr().read().tdre().bit_is_clear() {}
            self.rsci.tdr().write(|w| unsafe { w.tdat().bits(b as u16) });
        }
        Ok(buf.len())
    }

    /// Waits until the last byte has left the shift register (TEND).
    fn flush(&mut self) -> Result<(), Error> {
        while self.rsci.ssr().read().tend().bit_is_clear() {}
        Ok(())
    }
}

impl WriteReady for Serial {
    fn write_ready(&mut self) -> Result<bool, Error> {
        Ok(self.rsci.ssr().read().tdre().bit_is_set())
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
        Ok(self.pending.is_some() || self.rx_pending())
    }
}

#[cfg(feature = "async")]
pub use asynch::{AsyncSerial, on_groupal1, on_rxi12, on_txi12};

/// The interrupt-driven RSCI12 (feature `async`): `embedded-io-async`
/// `Read` / `Write`.
///
/// - **Receiving:** RXI12, routed to vector 144 (SLIR144), puts each byte
///   into a 128-byte ring buffer and wakes the reader. A receive error
///   comes as ERI12, in group interrupt AL1 (vector 105, GRPAL1_S.IS7): the
///   handler clears the error and keeps it, with its place in the stream:
///   `read` returns the bytes before it, then the error, then the bytes
///   after it. A full buffer drops the byte and counts as an overrun.
/// - **Transmitting:** a byte goes into TDR when TDRE is set; otherwise the
///   writer sets SCR0.TIE and sleeps until TXI12 (vector 145, SLIR145)
///   says TDR is empty again.
///
/// The three vectors belong to this driver: register its handlers with
/// [`bind_serial_interrupts!`](crate::bind_serial_interrupts), and leave
/// group AL1's other sources off.
#[cfg(feature = "async")]
mod asynch {
    use core::cell::RefCell;
    use core::future::poll_fn;
    use core::ptr::{read_volatile, write_volatile};
    use core::task::{Poll, Waker};

    use critical_section::Mutex;
    use embedded_io::ErrorType;

    use super::{Error, Serial};
    use crate::icu::{Interrupt, Priority};
    use crate::pac;

    /// The vectors the selectable interrupts RXI12 / TXI12 are routed to.
    const RXI_VECTOR: u8 = 144;
    const TXI_VECTOR: u8 = 145;
    /// Group interrupt AL1 (secure), which has ERI12.
    const GROUPAL1_VECTOR: u8 = 105;
    /// SLIR144 / SLIR145 take the event numbers of the sources.
    const SLIR144: *mut u16 = 0x8700_6920usize as _;
    const SLIR145: *mut u16 = 0x8700_6922usize as _;
    const EVENT_RXI12: u16 = 0x2ac;
    const EVENT_TXI12: u16 = 0x2ad;
    /// GRPAL1_S (request status) / GENAL1_S (enable): ERI12 is bit 7.
    const GRPAL1: *const u32 = 0x8700_6744usize as _;
    const GENAL1: *mut u32 = 0x8700_67c4usize as _;
    const ERI12: u32 = 1 << 7;

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
    static TX_WAKER: Mutex<RefCell<Option<Waker>>> = Mutex::new(RefCell::new(None));

    fn rsci() -> pac::Rsci12 {
        // The handlers and AsyncSerial share RSCI12; SCR0 is changed in
        // critical sections only.
        unsafe { pac::Rsci12::steal() }
    }

    fn wake(w: &mut Option<Waker>) {
        if let Some(w) = w.take() {
            w.wake();
        }
    }

    /// The interrupt-driven RSCI12; see the [module](self).
    pub struct AsyncSerial {
        serial: Serial,
    }

    impl Serial {
        /// Routes RXI12 / TXI12 to vectors 144 / 145, enables them and
        /// ERI12 (group AL1, vector 105) at `priority`, and returns the
        /// interrupt-driven serial port. Register the handlers with
        /// [`bind_serial_interrupts!`](crate::bind_serial_interrupts).
        pub fn into_async(self, priority: Priority) -> AsyncSerial {
            critical_section::with(|cs| {
                let mut rx = RX.borrow_ref_mut(cs);
                rx.head = 0;
                rx.len = 0;
                rx.error = self.pending.map(|e| (e, 0));
                rx.waker = None;
            });
            unsafe {
                write_volatile(SLIR144, EVENT_RXI12);
                write_volatile(SLIR145, EVENT_TXI12);
            }
            critical_section::with(|_| unsafe {
                write_volatile(GENAL1, read_volatile(GENAL1) | ERI12);
            });
            for irq in [rxi(), txi(), groupal1()] {
                irq.clear_pending();
                irq.set_priority(priority);
                irq.enable();
            }
            critical_section::with(|_| self.rsci.scr0().modify(|_, w| w.rie().set_bit()));
            AsyncSerial { serial: self }
        }
    }

    fn rxi() -> Interrupt {
        unsafe { Interrupt::new(RXI_VECTOR, RXI_VECTOR) }
    }
    fn txi() -> Interrupt {
        unsafe { Interrupt::new(TXI_VECTOR, TXI_VECTOR) }
    }
    fn groupal1() -> Interrupt {
        unsafe { Interrupt::new(GROUPAL1_VECTOR, GROUPAL1_VECTOR) }
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
        /// Writes the whole buffer, sleeping while TDR is full.
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
            let r = &self.serial.rsci;
            for &b in buf {
                poll_fn(|cx| {
                    if r.ssr().read().tdre().bit_is_set() {
                        return Poll::Ready(());
                    }
                    critical_section::with(|cs| {
                        *TX_WAKER.borrow_ref_mut(cs) = Some(cx.waker().clone());
                        r.scr0().modify(|_, w| w.tie().set_bit());
                    });
                    // TDR may have emptied before TIE was set.
                    if r.ssr().read().tdre().bit_is_set() {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                })
                .await;
                r.tdr().write(|w| unsafe { w.tdat().bits(b as u16) });
            }
            Ok(buf.len())
        }

        /// Waits until the last byte has left the shift register (TEND),
        /// yielding meanwhile (at most a character time).
        async fn flush(&mut self) -> Result<(), Error> {
            let r = &self.serial.rsci;
            poll_fn(|cx| {
                if r.ssr().read().tend().bit_is_set() {
                    Poll::Ready(Ok(()))
                } else {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            })
            .await
        }
    }

    /// The RXI12 handler (vector 144): buffers the received byte.
    pub extern "C" fn on_rxi12() {
        let r = rsci();
        critical_section::with(|cs| {
            let mut rx = RX.borrow_ref_mut(cs);
            while r.ssr().read().rdrf().bit_is_set() {
                let b = r.rdr().read().rdat().bits() as u8;
                if rx.len < RX_BUF {
                    let at = (rx.head + rx.len) % RX_BUF;
                    rx.buf[at] = b;
                    rx.len += 1;
                } else {
                    rx.set_error(Error::Overrun);
                }
            }
            wake(&mut rx.waker);
        })
    }

    /// The TXI12 handler (vector 145): TDR is empty; wakes the writer.
    pub extern "C" fn on_txi12() {
        let r = rsci();
        critical_section::with(|cs| {
            r.scr0().modify(|_, w| w.tie().clear_bit());
            wake(&mut TX_WAKER.borrow_ref_mut(cs));
        })
    }

    /// The group AL1 handler (vector 105): a receive error (ERI12) is
    /// cleared and kept for the next `read`.
    pub extern "C" fn on_groupal1() {
        if unsafe { read_volatile(GRPAL1) } & ERI12 == 0 {
            return;
        }
        let r = rsci();
        critical_section::with(|cs| {
            let ssr = r.ssr().read();
            let e = if ssr.orer().bit_is_set() {
                Error::Overrun
            } else if ssr.afer().bit_is_set() {
                Error::Framing
            } else {
                Error::Parity
            };
            let _ = r.rdr().read().bits();
            r.sscr().write(|w| {
                w.rdrfc().set_bit().aferc().set_bit().aperc().set_bit().orerc().set_bit()
            });
            let mut rx = RX.borrow_ref_mut(cs);
            rx.set_error(e);
            wake(&mut rx.waker);
        })
    }
}

/// Registers the interrupt-driven RSCI12's handlers (feature `async`) with
/// rx-rt's `interrupt!`: group AL1 (105), RXI12 (144), TXI12 (145). Use it
/// once, in a crate that depends on rx-rt.
#[cfg(feature = "async")]
#[macro_export]
macro_rules! bind_serial_interrupts {
    () => {
        rx_rt::interrupt!(105, $crate::serial::on_groupal1);
        rx_rt::interrupt!(144, $crate::serial::on_rxi12);
        rx_rt::interrupt!(145, $crate::serial::on_txi12);
    };
}
