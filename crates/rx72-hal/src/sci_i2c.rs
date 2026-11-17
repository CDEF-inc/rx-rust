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
    pub fn sci6<DM, CM>(
        sci: pac::Sci6,
        sda: Pin<0x00, 0, DM>,
        scl: Pin<0x00, 1, CM>,
        clocks: &Clocks,
    ) -> Self {
        let rate = Rate::new(clocks.pclkb, 100_000).expect("PCLKB out of range");
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
        SciI2c { sci, rate, polls: clocks.iclk / 1000, sda, scl }
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

    fn wait(&self, done: impl Fn() -> bool) -> Result<(), Error> {
        (0..=self.polls).any(|_| done()).then_some(()).ok_or(Error::Timeout)
    }

    /// SIMR2.IICACKT: the ACK (`nack` = false) or NACK sent for a received
    /// byte; 1 also while receiving an ACK.
    fn set_nack(&self, nack: bool) {
        self.sci.simr2().write(|w| unsafe { w.bits(simr2(nack)) });
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

    /// Receives a byte with the ACK or NACK set by `set_nack`: a dummy FFh
    /// to TDR, RXI6 for the byte, then TXI6 for the end of the frame.
    fn receive(&mut self) -> Result<u8, Error> {
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
        let mut started = false;
        for (read, run, _) in runs(ops) {
            self.condition(if started { RSTAREQ } else { STAREQ })?;
            started = true;
            // The ACK of the address is received (IICACKT = 1).
            self.set_nack(true);
            if !self.send(addr << 1 | read as u8)? {
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
                    *b = self.receive()?;
                }
                self.sci.scr().write(|w| unsafe { w.bits(SCR_ON) });
            } else {
                for op in run.iter() {
                    if let Operation::Write(w) = op {
                        for &b in w.iter() {
                            if !self.send(b)? {
                                return Err(Error::NoAcknowledge(NoAcknowledgeSource::Data));
                            }
                        }
                    }
                }
            }
        }
        if started {
            self.condition(STPREQ)?;
        }
        Ok(())
    }

    /// After an error: a stop condition; if that fails, the release of
    /// 42.7.7 (TE = RE = 0, SIMR3 = F0h, RDR read, TE = RE = 1).
    fn recover(&mut self) {
        self.sci.scr().write(|w| unsafe { w.bits(SCR_ON) });
        self.set_nack(true);
        if self.condition(STPREQ).is_err() {
            self.sci.scr().write(|w| unsafe { w.bits(0) });
            self.sci.simr3().write(|w| unsafe { w.bits(RELEASED) });
            let _ = self.sci.rdr().read().bits();
            self.sci.scr().write(|w| unsafe { w.bits(SCR_ON) });
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
        let r = self.run(address, operations);
        if r.is_err() {
            self.recover();
        }
        r
    }
}
