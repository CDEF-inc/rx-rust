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
//! RDR holds one (checked on the board). SSR gives TEND and the errors.
//!
//! RXD2 gets the internal pull-up, so that an unconnected CN8 idles high
//! instead of floating into framing errors.

use core::fmt;
use core::ptr::{read_volatile, write_volatile};

use embedded_io::{ErrorKind, ErrorType, Read, ReadReady, Write, WriteReady};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::mstp::{self, Mstpcr};
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
    _tx: Pin<0x01, 3, Function>,
    _rx: Pin<0x01, 2, Function>,
}

impl Serial {
    /// Sets up SCI2 (taking it out of module stop) at `baud` bit/s on
    /// P13 / P12, receiving and transmitting.
    ///
    /// Panics if no setting is within 3% of `baud`.
    pub fn sci2<TM, RM>(
        sci: pac::Sci2,
        tx: Pin<0x01, 3, TM>,
        rx: Pin<0x01, 2, RM>,
        baud: u32,
        clocks: &Clocks,
    ) -> Self {
        let s = BaudSettings::new(clocks.pclkb, baud).expect("baud rate out of range");
        assert!((s.baud as u64).abs_diff(baud as u64) * 100 <= 3 * baud as u64);
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
        // Wait at least one bit time before enabling (as the manual asks).
        let mut n = clocks.iclk / s.baud;
        while n > 0 {
            core::hint::spin_loop();
            n -= 1;
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
        Serial { sci, settings: s, pending: None, _tx: tx, _rx: rx }
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
    /// Writes the whole buffer, waiting for TXI2 before each byte.
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        for &b in buf {
            while !self.tx_ready() {}
            unsafe { write_volatile(IR_TXI2, 0) };
            self.sci.tdr().write(|w| unsafe { w.bits(b) });
        }
        Ok(buf.len())
    }

    /// Waits until the last byte has left the shift register (TEND).
    fn flush(&mut self) -> Result<(), Error> {
        while self.ssr() & TEND == 0 {}
        Ok(())
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
