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
