//! I2C master on RIIC0, on P12 = SCL0 and P13 = SDA0, as a blocking
//! embedded-hal `I2c` (7-bit addresses). On the AP-RX72M-0A these pins are
//! the serial connector CN8's RXD (pin 1) and TXD (pin 2), next to VDD
//! (pin 5) and GND (pin 6), so an I2C device plugs in there; the board has
//! no I2C device on them (its EEPROM is on SCI6's P00 / P01).
//!
//! The driver polls ICSR2 (manual 43.3.3 and 43.3.4, with ICIER = 0):
//! - It sends one byte at a time and waits for its acknowledge (TEND or
//!   NACKF) before it writes the next, so a NACK is known to be for the
//!   address or for a data byte. ICFER.NACKE stops the transfer on a NACK,
//!   and the driver sends a STOP.
//! - It receives with ICMR3.WAIT = 1: SCL is held low after each byte until
//!   ICDRR is read, so the driver can set the NACK of the last byte
//!   (ICMR3.ACKBT, behind ACKWP) and request the STOP or the repeated START
//!   before it releases the bus. A read of no bytes reads one and drops it,
//!   since the last byte must be NACKed.
//!
//! The bus clock IICφ is PCLKB / 2^ICMR1.CKS. The SCL high and low widths
//! (ICBRH / ICBRL + 1 cycles, 5 bits each; table 43.5) are computed so that
//! they meet the I2C-bus minimums with a margin, as rx74-hal's RI3C driver
//! does: Standard mode 4.1 / 6.0 us (at most 100 kHz), Fast mode 0.7 /
//! 1.8 us (at most 400 kHz). The bus free time after a STOP is the low
//! width, above the 4.7 / 1.3 us minimums. The digital noise filter (one
//! stage) and the SCL synchronization stay on, as after reset. A stuck SCL
//! ends a transfer through the timeout (ICFER.TMOE, the 16-bit long mode).
//!
//! The pins need pull-ups. The driver leaves PCR as the pins had it: pins
//! from `into_pull_up_input` keep the internal pull-ups (the pins are
//! inputs while the RIIC does not pull them low), which carry a short bus
//! at Standard mode.

use embedded_hal::i2c::{
    ErrorKind, ErrorType, I2c as I2cTrait, NoAcknowledgeSource, Operation, SevenBitAddress,
};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::mstp::{self, Mstpcr};
use crate::pac;

// ICCR1
const ICE: u8 = 1 << 7;
const IICRST: u8 = 1 << 6;
/// SOWP: written 1 so that SCLO / SDAO keep their value.
const SOWP: u8 = 1 << 4;
// ICCR2
const BBSY: u8 = 1 << 7;
const MST: u8 = 1 << 6;
const SP: u8 = 1 << 3;
const RS: u8 = 1 << 2;
const ST: u8 = 1 << 1;
// ICMR1.BCWP: written 1 so that the bit counter keeps its value.
const BCWP: u8 = 1 << 3;
// ICMR2: TMOH and TMOL (count while SCL is high and low), long mode
const TMO_HL: u8 = 0b110;
// ICMR3
const WAIT: u8 = 1 << 6;
const ACKWP: u8 = 1 << 4;
const ACKBT: u8 = 1 << 3;
// ICFER: SCLE, NFE, NACKE, MALE, TMOE
const ICFER: u8 = 0x73;
// ICSR2
const TDRE: u8 = 1 << 7;
const TEND: u8 = 1 << 6;
const RDRF: u8 = 1 << 5;
const NACKF: u8 = 1 << 4;
const STOP: u8 = 1 << 3;
const START: u8 = 1 << 2;
const AL: u8 = 1 << 1;
const TMOF: u8 = 1 << 0;
/// The reserved bits of ICBRH / ICBRL, written 1.
const BR_RESERVED: u8 = 0xe0;

/// PFS.PSEL of SCL0 / SDA0.
const PSEL_RIIC: u8 = 0b00_1111;

/// The bus speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    /// Standard mode, up to 100 kHz.
    Standard,
    /// Fast mode, up to 400 kHz.
    Fast,
}

/// The SCL timing settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// ICMR1.CKS: IICφ is PCLKB / 2^cks.
    pub cks: u8,
    /// ICBRH / ICBRL: the high and low widths are these + 1 IICφ cycles.
    pub brh: u8,
    pub brl: u8,
}

impl Timing {
    /// The settings for `speed` with PCLKB = `pclkb`: the smallest CKS
    /// whose low width fits in ICBRL's 5 bits.
    pub fn new(pclkb: u32, speed: Speed) -> Option<Self> {
        let (high_ns, low_ns) = match speed {
            Speed::Standard => (4_100, 6_000),
            Speed::Fast => (700, 1_800),
        };
        for cks in 0..8u8 {
            let clk = (pclkb >> cks) as u64;
            let counts = |ns: u64| (ns * clk).div_ceil(1_000_000_000);
            let (high, low) = (counts(high_ns), counts(low_ns));
            // The noise filter (one stage) needs ICBRH / ICBRL >= 2.
            if low <= 32 {
                return Some(Timing { cks, brh: high.max(3) as u8 - 1, brl: low.max(3) as u8 - 1 });
            }
        }
        None
    }

    /// The SCL frequency without the rise and fall times, in Hz.
    pub fn hz(&self, pclkb: u32) -> u32 {
        (pclkb >> self.cks) / (self.brh as u32 + self.brl as u32 + 2)
    }
}

/// An I2C error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// No ACK for the address or a data byte (ICSR2.NACKF).
    NoAcknowledge(NoAcknowledgeSource),
    /// Another master took the bus (ICSR2.AL).
    ArbitrationLoss,
    /// SCL stuck (ICSR2.TMOF), or the bus stayed busy (ICCR2.BBSY) for
    /// about 20 ms before a START.
    Timeout,
}

impl embedded_hal::i2c::Error for Error {
    fn kind(&self) -> ErrorKind {
        match *self {
            Error::NoAcknowledge(s) => ErrorKind::NoAcknowledge(s),
            Error::ArbitrationLoss => ErrorKind::ArbitrationLoss,
            Error::Timeout => ErrorKind::Other,
        }
    }
}

/// A RIIC reset (ICCR1.IICRST with ICE = 0): the registers to their reset
/// values, BBSY cleared, the pins released.
fn reset(riic: &pac::Riic0) {
    riic.iccr1().write(|w| unsafe { w.bits(SOWP) });
    riic.iccr1().write(|w| unsafe { w.bits(IICRST | SOWP) });
}

/// The set-up after a RIIC reset (ICCR1.IICRST with ICE = 0): the internal
/// reset, with the pins driven, during the register settings.
fn configure(riic: &pac::Riic0, timing: Timing) {
    riic.iccr1().write(|w| unsafe { w.bits(ICE | IICRST | SOWP) });
    // No slave addresses (the reset value matches the general call and
    // SARL0 = 0).
    riic.icser().write(|w| unsafe { w.bits(0) });
    riic.icmr1().write(|w| unsafe { w.bits(timing.cks << 4 | BCWP) });
    riic.icbrh().write(|w| unsafe { w.bits(BR_RESERVED | timing.brh) });
    riic.icbrl().write(|w| unsafe { w.bits(BR_RESERVED | timing.brl) });
    riic.icmr2().write(|w| unsafe { w.bits(TMO_HL) });
    // ACKBT writable from now on, ACK, no WAIT, one filter stage.
    riic.icmr3().write(|w| unsafe { w.bits(ACKWP) });
    riic.icfer().write(|w| unsafe { w.bits(ICFER) });
    riic.icier().write(|w| unsafe { w.bits(0) });
    riic.iccr1().write(|w| unsafe { w.bits(ICE | SOWP) });
}

/// RIIC0 as an I2C master.
pub struct I2c {
    riic: pac::Riic0,
    timing: Timing,
    /// Polls of BBSY before a START (each takes several PCLKB cycles).
    busy_polls: u32,
    scl: Pin<0x01, 2, Function>,
    sda: Pin<0x01, 3, Function>,
}

impl I2c {
    /// Takes RIIC0 out of module stop (MSTPCRB.MSTPB21) and starts it as an
    /// I2C master on P12 / P13 at `speed` (manual figure 43.4). The timing
    /// fits any PCLKB up to 682 MHz (it is at most 60 MHz).
    pub fn riic0<CM, DM>(
        riic: pac::Riic0,
        scl: Pin<0x01, 2, CM>,
        sda: Pin<0x01, 3, DM>,
        speed: Speed,
        clocks: &Clocks,
    ) -> Self {
        let timing = Timing::new(clocks.pclkb, speed).expect("PCLKB too fast");
        mstp::start(Mstpcr::B, 21);
        // A RIIC reset with the pins not driven, then the pins.
        reset(&riic);
        let scl = scl.into_function(PSEL_RIIC);
        let sda = sda.into_function(PSEL_RIIC);
        configure(&riic, timing);
        I2c { riic, timing, busy_polls: clocks.iclk / 1000, scl, sda }
    }

    /// The SCL timing in use.
    pub fn timing(&self) -> Timing {
        self.timing
    }

    /// Turns RIIC0 off (ICCR1.ICE = 0, the pins not driven) and gives it
    /// and its pins back.
    #[allow(clippy::type_complexity)]
    pub fn free(self) -> (pac::Riic0, Pin<0x01, 2, Function>, Pin<0x01, 3, Function>) {
        self.riic.iccr1().write(|w| unsafe { w.bits(SOWP) });
        (self.riic, self.scl, self.sda)
    }

    fn sr2(&self) -> u8 {
        self.riic.icsr2().read().bits()
    }

    fn cr2(&self) -> u8 {
        self.riic.iccr2().read().bits()
    }

    /// Requests ST / RS / SP (MST and TRS are write-protected: ICMR1.MTWP
    /// is 0).
    fn request(&self, bit: u8) {
        self.riic.iccr2().write(|w| unsafe { w.bits(bit) });
    }

    /// Clears ICSR2 flags (they take only 0 writes; 1 leaves a flag as it
    /// is).
    fn clear(&self, flags: u8) {
        self.riic.icsr2().write(|w| unsafe { w.bits(!flags) });
    }

    /// ICMR3 with ACKWP set: WAIT and the ACK (`nack` = false) or NACK.
    fn set_icmr3(&self, wait: bool, nack: bool) {
        let v = ACKWP | if wait { WAIT } else { 0 } | if nack { ACKBT } else { 0 };
        self.riic.icmr3().write(|w| unsafe { w.bits(v) });
    }

    /// Waits for any of `flags` in ICSR2, and ends with an error on AL,
    /// TMOF or NACKF (NACKF first: it comes with TEND or RDRF in the same
    /// read).
    fn wait(&self, flags: u8) -> Result<(), Error> {
        loop {
            let s = self.sr2();
            if s & AL != 0 {
                return Err(Error::ArbitrationLoss);
            }
            if s & TMOF != 0 {
                return Err(Error::Timeout);
            }
            if s & NACKF != 0 {
                return Err(Error::NoAcknowledge(NoAcknowledgeSource::Unknown));
            }
            if s & flags != 0 {
                return Ok(());
            }
        }
    }

    /// Requests a START (or a repeated START) and sends the address byte,
    /// then waits for its acknowledge: TEND for a write, RDRF for a read.
    /// A read that ended with `then_restart` has already requested the
    /// repeated START (`requested`), so it is only waited for.
    fn address(
        &mut self,
        addr: u8,
        read: bool,
        restart: bool,
        requested: bool,
    ) -> Result<(), Error> {
        if restart {
            if !requested {
                self.clear(START);
                self.request(RS);
            }
        } else {
            let mut polls = self.busy_polls;
            while self.cr2() & BBSY != 0 {
                polls = polls.checked_sub(1).ok_or(Error::Timeout)?;
            }
            self.clear(START);
            self.request(ST);
        }
        self.wait(START)?;
        self.wait(TDRE)?;
        self.riic.icdrt().write(|w| unsafe { w.bits(addr << 1 | read as u8) });
        self.wait(if read { RDRF } else { TEND }).map_err(|e| no_ack(e, true))
    }

    /// Sends `bytes`, one at a time: each is written once the previous one
    /// was acknowledged (TEND), which ICDRT's write clears.
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Error> {
        for &b in bytes {
            self.riic.icdrt().write(|w| unsafe { w.bits(b) });
            self.wait(TEND).map_err(|e| no_ack(e, false))?;
        }
        Ok(())
    }

    /// Receives into the read operations `ops` (one run) after the address
    /// byte, then requests a STOP, or a repeated START if `then_restart`.
    fn read_bytes(&mut self, ops: &mut [Operation<'_>], then_restart: bool) -> Result<(), Error> {
        let wanted = read_len(ops);
        // A read of nothing still reads (and drops) one byte.
        let total = wanted.max(1);
        let mut dropped = 0u8;
        // SCL held low after each byte until ICDRR is read; NACK for the
        // last byte. The dummy read starts the first byte.
        self.set_icmr3(true, total == 1);
        let _ = self.riic.icdrr().read().bits();
        let bytes = bytes_to_read(ops).chain((wanted == 0).then_some(&mut dropped));
        for (k, b) in bytes.enumerate() {
            self.wait(RDRF)?;
            let n = k + 1;
            if n == total {
                // The end, requested before the last read releases SCL.
                if then_restart {
                    self.clear(START);
                    self.request(RS);
                } else {
                    self.clear(STOP);
                    self.request(SP);
                }
            } else if n == total - 1 {
                // This read releases the last byte: NACK it.
                self.set_icmr3(true, true);
            }
            *b = self.riic.icdrr().read().bits();
        }
        // WAIT off after the last read (43.2.5), and ACK for the next read.
        self.set_icmr3(false, false);
        Ok(())
    }

    /// Requests a STOP (unless one is already on its way) and waits for it.
    /// After the address of a read was NACKed, a dummy read of ICDRR
    /// releases SCL for it (figure 43.9); after a write it changes nothing,
    /// so it is always done. A timeout (SCL held low) leaves TMOF set for
    /// `recover`.
    fn stop(&mut self, requested: bool) -> Result<(), Error> {
        if !requested {
            self.clear(STOP);
            self.request(SP);
            let _ = self.riic.icdrr().read().bits();
        }
        loop {
            let s = self.sr2();
            if s & STOP != 0 {
                break;
            }
            if s & TMOF != 0 {
                return Err(Error::Timeout);
            }
        }
        self.clear(STOP | NACKF | START);
        Ok(())
    }

    /// After an error: a STOP if RIIC0 is still the master, else (or if
    /// that STOP times out) a RIIC reset and the set-up again. Only the RIIC
    /// reset clears BBSY (an internal reset keeps it), so a bus seen busy
    /// through a stuck SCL is free again for the next transfer.
    fn recover(&mut self) {
        let s = self.sr2();
        if s & (AL | TMOF) != 0 || self.cr2() & MST == 0 || self.stop(false).is_err() {
            reset(&self.riic);
            configure(&self.riic, self.timing);
            return;
        }
        self.clear(NACKF | AL | TMOF | START | STOP);
        self.set_icmr3(false, false);
    }

    fn run(&mut self, addr: u8, ops: &mut [Operation<'_>]) -> Result<(), Error> {
        let mut started = false;
        // Set when the previous read run requested the STOP or the
        // repeated START itself.
        let mut end_requested = false;
        for (read, run, last) in runs(ops) {
            self.address(addr, read, started, end_requested)?;
            if read {
                self.read_bytes(run, !last)?;
            } else {
                for op in run.iter() {
                    if let Operation::Write(w) = op {
                        self.write_bytes(w)?;
                    }
                }
            }
            end_requested = read;
            started = true;
        }
        if started {
            self.stop(end_requested)?;
        }
        Ok(())
    }
}

fn is_read(op: &Operation<'_>) -> bool {
    matches!(op, Operation::Read(_))
}

/// The operations of a transaction in runs of one direction: whether the
/// run reads, the run, and whether it is the last one.
pub(crate) fn runs<'a, 'b>(
    ops: &'a mut [Operation<'b>],
) -> impl Iterator<Item = (bool, &'a mut [Operation<'b>], bool)> {
    let mut it = ops.chunk_by_mut(|a, b| is_read(a) == is_read(b)).peekable();
    core::iter::from_fn(move || {
        let run = it.next()?;
        Some((is_read(&run[0]), run, it.peek().is_none()))
    })
}

/// The number of bytes the read operations of `ops` take.
pub(crate) fn read_len(ops: &[Operation<'_>]) -> usize {
    ops.iter().map(|op| if let Operation::Read(b) = op { b.len() } else { 0 }).sum()
}

/// The bytes of the read operations of `ops`, in order.
pub(crate) fn bytes_to_read<'a>(ops: &'a mut [Operation<'_>]) -> impl Iterator<Item = &'a mut u8> {
    ops.iter_mut().flat_map(|op| match op {
        Operation::Read(b) => &mut b[..],
        Operation::Write(_) => &mut [][..],
    })
}

/// A NACK on the address byte becomes NoAcknowledge(Address), on data
/// NoAcknowledge(Data).
fn no_ack(e: Error, address: bool) -> Error {
    match e {
        Error::NoAcknowledge(_) if address => Error::NoAcknowledge(NoAcknowledgeSource::Address),
        Error::NoAcknowledge(_) => Error::NoAcknowledge(NoAcknowledgeSource::Data),
        e => e,
    }
}

impl ErrorType for I2c {
    type Error = Error;
}

impl I2cTrait<SevenBitAddress> for I2c {
    fn transaction(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        let r = self.run(address, operations);
        if r.is_err() {
            self.recover();
        }
        r
    }
}
