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
//! a wait that does not end in about 1 ms is `Error::Timeout`, after which
//! the bus is released as in 42.7.7.

use core::fmt;
use core::ptr::{read_volatile, write_volatile};

use embedded_hal::i2c::{
    ErrorKind, ErrorType, I2c as I2cTrait, NoAcknowledgeSource, Operation, SevenBitAddress,
};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::mstp::{self, Mstpcr};
use crate::pac;

/// PFS.PSEL of SSDA6 / SSCL6 (the SCI function, as TXD6 / RXD6).
const PSEL_SCI: u8 = 0b01010;
/// PORT0.ODR0: B0 (P00) and B2 (P01) select N-channel open drain.
const ODR0_PORT0: *mut u8 = 0x0008_c080 as _;
const ODR_P00_P01: u8 = 1 << 0 | 1 << 2;

// ICU interrupt request flags of RXI6 / TXI6 (vectors 86 / 87).
const IR_RXI6: *mut u8 = 0x0008_7056 as _;
const IR_TXI6: *mut u8 = 0x0008_7057 as _;

// SCR
const TIE: u8 = 1 << 7;
const RIE: u8 = 1 << 6;
const TE: u8 = 1 << 5;
const RE: u8 = 1 << 4;
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
    /// A condition or a frame did not complete in about 1 ms (SCL held low).
    Timeout,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoAcknowledge(s) => write!(f, "no acknowledge ({s})"),
            Error::Timeout => f.write_str("timeout"),
        }
    }
}

impl core::error::Error for Error {}

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
    _sda: Pin<0x00, 0, Function>,
    _scl: Pin<0x00, 1, Function>,
}

impl SciI2c {
    /// Sets up SCI6 (taking it out of module stop) as a simple I2C master
    /// at 100 kHz or just under (98.7 kHz at PCLKB 60 MHz) on P00 = SSDA6 /
    /// P01 = SSCL6, N-channel open drain (manual 42.7.4).
    pub fn sci6<DM, CM>(
        sci: pac::Sci6,
        sda: Pin<0x00, 0, DM>,
        scl: Pin<0x00, 1, CM>,
        clocks: &Clocks,
    ) -> Self {
        let rate = Rate::new(clocks.pclkb, 100_000).expect("PCLKB out of range");
        mstp::start(Mstpcr::B, 25);
        sci.scr().write(|w| unsafe { w.bits(0) });
        critical_section::with(|_| unsafe {
            write_volatile(ODR0_PORT0, read_volatile(ODR0_PORT0) | ODR_P00_P01);
        });
        let sda = sda.into_function(PSEL_SCI);
        let scl = scl.into_function(PSEL_SCI);
        sci.simr3().write(|w| unsafe { w.bits(RELEASED) });
        sci.smr().write(|w| unsafe { w.bits(rate.cks) });
        sci.scmr().write(|w| unsafe { w.bits(SCMR_I2C) });
        sci.brr().write(|w| unsafe { w.bits(rate.brr) });
        sci.semr().write(|w| unsafe { w.bits(NFEN) });
        sci.snfr().write(|w| unsafe { w.bits(NFCS_1) });
        sci.simr1().write(|w| unsafe { w.bits(SIMR1_I2C) });
        sci.simr2().write(|w| unsafe { w.bits(IICACKT | IICCSC | IICINTM) });
        sci.spmr().write(|w| unsafe { w.bits(0) });
        sci.scr().write(|w| unsafe { w.bits(TIE | TE | RE) });
        let polls = clocks.iclk / 1000;
        SciI2c { sci, rate, polls, _sda: sda, _scl: scl }
    }

    /// The bit rate setting in use.
    pub fn rate(&self) -> Rate {
        self.rate
    }

    /// Gives SCI6 and the pins back (SCI6 stopped, the pins left as they
    /// are).
    pub fn free(self) -> (pac::Sci6, Pin<0x00, 0, Function>, Pin<0x00, 1, Function>) {
        self.sci.scr().write(|w| unsafe { w.bits(0) });
        (self.sci, self._sda, self._scl)
    }

    fn wait(&self, done: impl Fn() -> bool) -> Result<(), Error> {
        for _ in 0..self.polls {
            if done() {
                return Ok(());
            }
        }
        if done() { Ok(()) } else { Err(Error::Timeout) }
    }

    /// Generates a start, restart or stop condition and waits for it
    /// (IICSTIF), then sets the pins to serial output, or released after a
    /// stop.
    fn condition(&mut self, req: u8) -> Result<(), Error> {
        self.sci.simr3().write(|w| unsafe { w.bits(req & !IICSTIF) });
        let sci = &self.sci;
        self.wait(|| sci.simr3().read().bits() & IICSTIF != 0)?;
        let after = if req == STPREQ { RELEASED } else { SERIAL };
        self.sci.simr3().write(|w| unsafe { w.bits(after) });
        Ok(())
    }

    /// Sends a frame (TDR), waits for its end (TXI6) and returns whether it
    /// was ACKed.
    fn send(&mut self, b: u8) -> Result<bool, Error> {
        unsafe { write_volatile(IR_TXI6, 0) };
        self.sci.tdr().write(|w| unsafe { w.bits(b) });
        self.wait(|| unsafe { read_volatile(IR_TXI6) } != 0)?;
        Ok(self.sci.sisr().read().bits() & IICACKR == 0)
    }

    /// Receives a byte, answering ACK, or NACK for the `last` one: a dummy
    /// FFh to TDR, RXI6 for the byte, then TXI6 for the end of the frame.
    fn receive(&mut self, last: bool) -> Result<u8, Error> {
        let ackt = if last { IICACKT } else { 0 };
        self.sci.simr2().write(|w| unsafe { w.bits(ackt | IICCSC | IICINTM) });
        unsafe {
            write_volatile(IR_RXI6, 0);
            write_volatile(IR_TXI6, 0);
        }
        self.sci.tdr().write(|w| unsafe { w.bits(0xff) });
        self.wait(|| unsafe { read_volatile(IR_RXI6) } != 0)?;
        let b = self.sci.rdr().read().bits();
        self.wait(|| unsafe { read_volatile(IR_TXI6) } != 0)?;
        Ok(b)
    }

    fn run(&mut self, addr: u8, ops: &mut [Operation<'_>]) -> Result<(), Error> {
        let mut i = 0;
        let mut started = false;
        while i < ops.len() {
            let read = matches!(ops[i], Operation::Read(_));
            // The run of operations in the same direction.
            let mut j = i;
            while j < ops.len() && matches!(ops[j], Operation::Read(_)) == read {
                j += 1;
            }
            self.condition(if started { RSTAREQ } else { STAREQ })?;
            started = true;
            // The ACK of the address is received (IICACKT = 1).
            self.sci.simr2().write(|w| unsafe { w.bits(IICACKT | IICCSC | IICINTM) });
            if !self.send(addr << 1 | read as u8)? {
                return Err(Error::NoAcknowledge(NoAcknowledgeSource::Address));
            }
            if read {
                self.sci.scr().write(|w| unsafe { w.bits(TIE | RIE | TE | RE) });
                let total: usize = ops[i..j]
                    .iter()
                    .map(|op| if let Operation::Read(r) = op { r.len() } else { 0 })
                    .sum();
                if total == 0 {
                    self.receive(true)?;
                }
                let mut n = 0;
                for op in &mut ops[i..j] {
                    if let Operation::Read(r) = op {
                        for b in r.iter_mut() {
                            n += 1;
                            *b = self.receive(n == total)?;
                        }
                    }
                }
                self.sci.scr().write(|w| unsafe { w.bits(TIE | TE | RE) });
            } else {
                for op in &ops[i..j] {
                    if let Operation::Write(w) = op {
                        for &b in w.iter() {
                            if !self.send(b)? {
                                return Err(Error::NoAcknowledge(NoAcknowledgeSource::Data));
                            }
                        }
                    }
                }
            }
            i = j;
        }
        if started {
            self.condition(STPREQ)?;
        }
        Ok(())
    }

    /// After an error: a stop condition; if that fails, the release of
    /// 42.7.7 (TE = RE = 0, SIMR3 = F0h, RDR read, TE = RE = 1).
    fn recover(&mut self) {
        self.sci.scr().write(|w| unsafe { w.bits(TIE | TE | RE) });
        self.sci.simr2().write(|w| unsafe { w.bits(IICACKT | IICCSC | IICINTM) });
        if self.condition(STPREQ).is_err() {
            self.sci.scr().write(|w| unsafe { w.bits(0) });
            self.sci.simr3().write(|w| unsafe { w.bits(RELEASED) });
            let _ = self.sci.rdr().read().bits();
            self.sci.scr().write(|w| unsafe { w.bits(TIE | TE | RE) });
        }
    }
}

impl ErrorType for SciI2c {
    type Error = Error;
}

impl I2cTrait<SevenBitAddress> for SciI2c {
    /// A read of no bytes reads one byte (with the NACK) and drops it: the
    /// NACK of the last byte is what ends a read in this mode.
    fn transaction(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        let r = self.run(address, operations);
        if r.is_err() {
            self.recover();
        }
        r
    }
}
