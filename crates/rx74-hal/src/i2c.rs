//! I2C master on RI3C0 in its I2C protocol mode, on P102 = SCL00-A and
//! P103 = SDA00-A, as a blocking embedded-hal `I2c` (7-bit addresses). On
//! the EK-RX74M these pins carry the on-board EtherCAT EEPROM (U13); the
//! EK-RX74N leaves them free.
//!
//! The RX74N / RX74M have no RIIC. RI3C has two modes: the I3C protocol
//! mode works through command and response queues, and its legacy I2C
//! messages only run at Fast-mode and faster; the I2C protocol mode
//! (ICMR.OMS = 1, the reset value) works like the RIIC, byte by byte
//! (manual 44.4.8): ICCGR requests START / repeated START / STOP, ICDR
//! holds one byte, and the driver polls ICCSR.TDRE / RDRF and the ICSR2
//! flags (START, STOP, NACKF, TEND, AL, TMOF), whose detection it enables
//! in ICSER / ICCSER. A read holds SCL after each byte (ICCSCR.WAITRE) so
//! that the driver can NACK the last byte and request the STOP or the
//! repeated START in time; reading ICDR releases the next byte.
//!
//! The bus clock is I3CCLK, set in the clock generation circuit (I3CCKCR /
//! I3CCKDIVCR, behind PRCR_S.PRC0) to PLL1P divided to at most 60 MHz, the
//! frequency of the manual's examples; its reset value is MOCO. The SCL
//! high and low widths (ICFBR, in I3CCLK / 2^ICRCCR.CKS cycles plus the
//! digital filter's two) are computed so that they meet the I2C-bus
//! minimums with a margin: Standard mode 4.1 / 6.0 us (at most 100 kHz),
//! Fast mode 0.7 / 1.8 us (at most 400 kHz). The manual's example values
//! give an SCL high slightly below the minimums. ICBFTR holds the bus free
//! time (4.7 / 1.3 us). A stuck SCL ends a transfer through the timeout
//! (ICTOR, 65536 cycles).
//!
//! Two corner cases: the manual gives no completion flag for a write of
//! no bytes (TEND excludes the address), so the driver waits 10 SCL
//! periods before it looks for the NACK; a read of no bytes reads one and
//! drops it, since the last byte must be NACKed.
//!
//! Registers at RI3C0's secure address (0x8745F000), addressed directly.

use embedded_hal::i2c::{
    ErrorKind, ErrorType, I2c as I2cTrait, NoAcknowledgeSource, Operation, SevenBitAddress,
};

use core::ptr::{read_volatile, write_volatile};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::mstp::{self, Mstpcr};
use crate::pac;
use crate::timeout;

const BASE: usize = 0x8745_f000;
const ICCCR: usize = BASE + 0x010;
const ICCR: usize = BASE + 0x014;
const ICRCR: usize = BASE + 0x020;
const ICRCCR: usize = BASE + 0x070;
const ICFBR: usize = BASE + 0x074;
const ICBFTR: usize = BASE + 0x07c;
const ICICR: usize = BASE + 0x08c;
const ICACKR: usize = BASE + 0x0a0;
const ICCSCR: usize = BASE + 0x0a4;
const ICCGR: usize = BASE + 0x140;
const ICDR: usize = BASE + 0x158;
const ICSR2: usize = BASE + 0x1d0;
const ICSER: usize = BASE + 0x1d4;
const ICCSR: usize = BASE + 0x1e0;
const ICCSER: usize = BASE + 0x1e4;
const ICBSR: usize = BASE + 0x210;

// ICCR / ICRCR
const ICE: u32 = 1 << 31;
const MRST: u32 = 1 << 0;
const ISRST: u32 = 1 << 16;
// ICCGR
const ST: u32 = 1 << 0;
const RS: u32 = 1 << 1;
const SP: u32 = 1 << 2;
// ICSR2
const START: u32 = 1 << 0;
const STOP: u32 = 1 << 1;
const NACKF: u32 = 1 << 4;
const TEND: u32 = 1 << 8;
const AL: u32 = 1 << 16;
const TMOF: u32 = 1 << 20;
// ICCSR
const TDRE: u32 = 1 << 0;
const RDRF: u32 = 1 << 1;
// ICACKR: ACKBT with its write enable ACKWP
const ACK: u32 = 0b100;
const NACK: u32 = 0b110;
// ICCSCR.WAITRE
const WAITRE: u32 = 1 << 1;

// The clock generation circuit's I3CCLK selection.
const I3CCKDIVCR: *mut u8 = 0x8701_9070_usize as _;
const I3CCKCR: *mut u8 = 0x8701_9078_usize as _;
const I3CCKSREQ: u8 = 1 << 6;
const I3CCKSRDY: u8 = 1 << 7;
const I3CCKSEL_PLL1P: u8 = 0b0101;
const PRCR_S: *mut u16 = 0x8701_93fa_usize as _;

/// I3CCKDIVCR.I3CCKDIV codes, smallest divider first.
const I3CCK_DIVS: [(u32, u8); 10] = [
    (1, 0b0000),
    (2, 0b0001),
    (3, 0b0101),
    (4, 0b0010),
    (5, 0b0110),
    (6, 0b0011),
    (8, 0b0100),
    (10, 0b0111),
    (16, 0b1000),
    (32, 0b1001),
];

/// PSEL of SCL00-A / SDA00-A.
const PSEL_RI3C: u8 = 0b001_0001;

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
    /// ICRCCR.CKS: the internal clock is I3CCLK / 2^cks.
    pub cks: u8,
    /// ICFBR.HIGH / LOW, in internal clock cycles (the filter adds 2).
    pub high: u8,
    pub low: u8,
    /// ICBFTR, the bus free time in internal clock cycles.
    pub bus_free: u16,
}

impl Timing {
    /// The settings for `speed` with I3CCLK = `i3cclk`: the smallest CKS
    /// whose low width fits in 8 bits.
    pub fn new(i3cclk: u32, speed: Speed) -> Option<Self> {
        let (high_ns, low_ns, buf_ns) = match speed {
            Speed::Standard => (4_100, 6_000, 4_700),
            Speed::Fast => (700, 1_800, 1_300),
        };
        for cks in 0..8u8 {
            let clk = (i3cclk >> cks) as u64;
            let counts = |ns: u64| (ns * clk).div_ceil(1_000_000_000);
            let (high, low, bus_free) = (counts(high_ns) - 2, counts(low_ns) - 2, counts(buf_ns));
            if low <= 255 && bus_free <= 511 {
                return Some(Timing {
                    cks,
                    high: high as u8,
                    low: low as u8,
                    bus_free: bus_free as u16,
                });
            }
        }
        None
    }
}

/// An I2C error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// No ACK for the address or a data byte (ICSR2.NACKF).
    NoAcknowledge(NoAcknowledgeSource),
    /// Another master took the bus (ICSR2.AL).
    ArbitrationLoss,
    /// SCL stuck (ICSR2.TMOF).
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

fn rd(a: usize) -> u32 {
    unsafe { read_volatile(a as *const u32) }
}

fn wr(a: usize, v: u32) {
    unsafe { write_volatile(a as *mut u32, v) }
}

/// Clears ICSR2 flags (they take only 0 writes; 1 leaves a flag as it is).
fn clear(flags: u32) {
    wr(ICSR2, !flags);
}

/// Why [`I2c::try_ri3c0`] failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SetupError {
    /// PLL1 is not running, or no I3CCLK divider and timing fit.
    Clock,
    /// I3CCLK's switch or the RI3C reset (ICRCR.MRST) did not end in 1 ms.
    Timeout,
}

impl core::fmt::Display for SetupError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            SetupError::Clock => "no I3CCLK from PLL1P",
            SetupError::Timeout => "RI3C set-up timeout",
        })
    }
}

impl core::error::Error for SetupError {}

/// RI3C0 as an I2C master.
pub struct I2c {
    ri3c: pac::Ri3c0,
    timing: Timing,
    /// CPUCLK0 cycles for 10 SCL periods (at least one per loop).
    ten_scl: u32,
    scl: Pin<0x01, 2, Function>,
    sda: Pin<0x01, 3, Function>,
}

impl I2c {
    /// Sets I3CCLK up from PLL1P, takes RI3C0 out of module stop
    /// (MSTPCRB.MSTPB4) and starts it as an I2C master on P102 / P103 at
    /// `speed` (manual figure 44.17). The bus needs external pull-ups.
    ///
    /// Panics where [`try_ri3c0`](Self::try_ri3c0) returns an error.
    pub fn ri3c0<CM, DM>(
        ri3c: pac::Ri3c0,
        scl: Pin<0x01, 2, CM>,
        sda: Pin<0x01, 3, DM>,
        speed: Speed,
        clocks: &Clocks,
    ) -> Self {
        Self::try_ri3c0(ri3c, scl, sda, speed, clocks).expect("RI3C set-up failed")
    }

    /// [`ri3c0`](Self::ri3c0), or [`SetupError::Clock`] if PLL1 is not
    /// running (nothing changed), or [`SetupError::Timeout`]. The
    /// peripheral and pins are dropped on an error.
    pub fn try_ri3c0<CM, DM>(
        ri3c: pac::Ri3c0,
        scl: Pin<0x01, 2, CM>,
        sda: Pin<0x01, 3, DM>,
        speed: Speed,
        clocks: &Clocks,
    ) -> Result<Self, SetupError> {
        let &(div, code) = I3CCK_DIVS
            .iter()
            .find(|&&(d, _)| clocks.pll1p > 0 && clocks.pll1p / d <= 60_000_000)
            .ok_or(SetupError::Clock)?;
        let i3cclk = clocks.pll1p / div;
        let timing = Timing::new(i3cclk, speed).ok_or(SetupError::Clock)?;
        let spins_1ms = timeout::spins(clocks.cpuclk, 1000);
        // One SCL period: high + low + 4 internal clock cycles, plus rise
        // and fall times (up to 1.3 us), rounded up generously.
        let period_ns = (timing.high as u64 + timing.low as u64 + 4) * 1_000_000_000
            / (i3cclk >> timing.cks) as u64
            + 1_300;
        let ten_scl = (clocks.cpuclk as u64 * period_ns * 10).div_ceil(1_000_000_000) as u32;
        set_i3cclk(code, spins_1ms)?;
        let scl = scl.into_function(PSEL_RI3C);
        let sda = sda.into_function(PSEL_RI3C);
        mstp::start(Mstpcr::B, 4);
        // CCE, the RI3C off, then a reset of all its registers.
        wr(ICCCR, 1);
        wr(ICCR, 0);
        wr(ICRCR, MRST);
        timeout::wait(spins_1ms, || rd(ICRCR) & MRST == 0).map_err(|_| SetupError::Timeout)?;
        // The reserved bits that read 1 are written 1.
        wr(ICRCCR, 1 << 16 | timing.cks as u32);
        wr(ICFBR, 0x3f3f_0000 | (timing.high as u32) << 8 | timing.low as u32);
        wr(ICBFTR, timing.bus_free as u32);
        // Analog and digital filters, 2 stages.
        wr(ICICR, 0xf1);
        // Detect START, STOP, NACK, TEND, AL, timeout; TDRE, RDRF.
        wr(ICSER, 0x0011_0113);
        wr(ICCSER, 0b11);
        wr(ICCR, ICE);
        Ok(I2c { ri3c, timing, ten_scl, scl, sda })
    }

    /// The SCL timing in use.
    pub fn timing(&self) -> Timing {
        self.timing
    }

    /// Turns RI3C0 off (ICCR.ICE = 0) and gives it and its pins back.
    #[allow(clippy::type_complexity)]
    pub fn free(self) -> (pac::Ri3c0, Pin<0x01, 2, Function>, Pin<0x01, 3, Function>) {
        wr(ICCR, 0);
        (self.ri3c, self.scl, self.sda)
    }

    /// Requests START (or repeated START) and sends the address byte. A
    /// read that ended with `then_restart` has already requested the
    /// repeated START (`requested`), so it is only waited for.
    async fn address(
        &mut self,
        w: &mut impl Waiter,
        addr: u8,
        read: bool,
        restart: bool,
        requested: bool,
    ) -> Result<(), Error> {
        if restart {
            if !requested {
                clear(START);
                wr(ICCGR, RS);
            }
        } else {
            while rd(ICBSR) & 1 == 0 {
                if rd(ICSR2) & (AL | TMOF) != 0 {
                    return Err(Error::Timeout);
                }
            }
            wr(ICCGR, ST);
        }
        w.until(0, START, ERRORS).await?;
        // Data written while RS is still 1 is not sent. START was detected,
        // so the request clears at once; 10 SCL periods is a fault.
        timeout::wait(self.ten_scl, || rd(ICCGR) & (ST | RS) == 0).map_err(|_| Error::Timeout)?;
        w.until(TDRE, 0, ERRORS).await?;
        wr(ICDR, (addr as u32) << 1 | read as u32);
        Ok(())
    }

    /// Sends `bytes`; `first` if they follow the address byte directly.
    ///
    /// TDRE comes back as soon as a byte moves to the shift register, so
    /// the next one is written before the previous one's ACK. On a NACK,
    /// the RI3C does not set TDRE again for a byte already written (manual
    /// 44.5.5.2): with TDRE = 0 the NACKed item is the one before the last
    /// byte written (the address if that was the first data byte), with
    /// TDRE = 1 the last byte itself.
    async fn write_bytes(&mut self, w: &mut impl Waiter, bytes: &[u8], first: bool) -> Result<(), Error> {
        if bytes.is_empty() {
            // No completion flag for the address alone: wait 10 SCL
            // periods, then look for its NACK. The volatile read keeps the
            // loop (core::hint::spin_loop is nothing on RX).
            let keep = 0u8;
            for _ in 0..self.ten_scl {
                unsafe { read_volatile(&keep) };
            }
            if rd(ICSR2) & NACKF != 0 {
                return Err(Error::NoAcknowledge(if first {
                    NoAcknowledgeSource::Address
                } else {
                    NoAcknowledgeSource::Data
                }));
            }
            return Ok(());
        }
        // The index of the last byte written; -1 for the address.
        let mut last: isize = -1;
        let nacked = |last: isize, e: Error| match e {
            Error::NoAcknowledge(_) => {
                let item = if rd(ICCSR) & TDRE != 0 { last } else { last - 1 };
                Error::NoAcknowledge(if first && item < 0 {
                    NoAcknowledgeSource::Address
                } else {
                    NoAcknowledgeSource::Data
                })
            }
            e => e,
        };
        for &b in bytes {
            w.until(TDRE, 0, ERRORS).await.map_err(|e| nacked(last, e))?;
            wr(ICDR, b as u32);
            last += 1;
        }
        w.until(0, TEND, ERRORS).await.map_err(|e| nacked(last, e))
    }

    /// Receives into the read operations `ops` (one run) after the address
    /// byte, then requests a STOP, or a repeated START if `then_restart`.
    async fn read_bytes(
        &mut self,
        w: &mut impl Waiter,
        ops: &mut [Operation<'_>],
        then_restart: bool,
    ) -> Result<(), Error> {
        let len = |op: &Operation<'_>| if let Operation::Read(b) = op { b.len() } else { 0 };
        let wanted: usize = ops.iter().map(len).sum();
        // A read of nothing still reads (and drops) one byte.
        let total = wanted.max(1);
        let mut dropped = [0u8; 1];
        // The address: RDRF once the target ACKed it (TRS = 0).
        w.until(RDRF, 0, ERRORS).await.map_err(|e| no_ack(e, true))?;
        wr(ICCSCR, WAITRE);
        wr(ICACKR, if total == 1 { NACK } else { ACK });
        // The dummy read starts the first byte.
        let _ = rd(ICDR);
        let mut n = 0;
        let mut bufs = ops.iter_mut().filter_map(|op| match op {
            Operation::Read(b) => Some(&mut **b),
            Operation::Write(_) => None,
        });
        let mut spare = [&mut dropped[..]];
        let bufs: &mut dyn Iterator<Item = &mut [u8]> =
            if wanted == 0 { &mut spare.iter_mut().map(|b| &mut **b) } else { &mut bufs };
        for buf in bufs {
            for b in buf.iter_mut() {
                w.until(RDRF, 0, ERRORS).await?;
                n += 1;
                if n == total {
                    // The end, requested before the last read releases SCL.
                    if then_restart {
                        clear(START);
                        wr(ICCGR, RS);
                    } else {
                        clear(STOP);
                        wr(ICCGR, SP);
                    }
                } else if n == total - 1 {
                    // This read releases the last byte: NACK it.
                    wr(ICACKR, NACK);
                }
                *b = rd(ICDR) as u8;
            }
        }
        wr(ICCSCR, 0);
        Ok(())
    }

    /// Requests a STOP (unless one is already on its way) and waits for it.
    /// A timeout (SCL held low) leaves TMOF set for `recover`.
    async fn stop(&mut self, w: &mut impl Waiter, requested: bool) -> Result<(), Error> {
        if !requested {
            clear(STOP);
            wr(ICCGR, SP);
        }
        // Only a timeout ends it: NACKF may still be set from the transfer.
        w.until(0, STOP, TMOF).await?;
        clear(STOP | NACKF | START | TEND);
        wr(ICACKR, ACK);
        Ok(())
    }

    /// After an error: a STOP if the RI3C is still master, else (or if
    /// that STOP times out) an internal reset of the flags and requests
    /// (ICRCR.ISRST).
    fn recover(&mut self) {
        let s = rd(ICSR2);
        wr(ICCSCR, 0);
        if s & (AL | TMOF) != 0 || block_on(self.stop(&mut Spin, false)).is_err() {
            wr(ICRCR, ISRST);
            wr(ICRCR, 0);
        }
        clear(NACKF | AL | TMOF | START | STOP | TEND);
        wr(ICACKR, ACK);
    }

    async fn run(&mut self, w: &mut impl Waiter, addr: u8, ops: &mut [Operation<'_>]) -> Result<(), Error> {
        let mut i = 0;
        let mut started = false;
        // Set when the previous read run requested the STOP or the
        // repeated START itself.
        let mut end_requested = false;
        while i < ops.len() {
            let read = matches!(ops[i], Operation::Read(_));
            // The run of operations in the same direction.
            let mut j = i;
            while j < ops.len() && matches!(ops[j], Operation::Read(_)) == read {
                j += 1;
            }
            self.address(w, addr, read, started, end_requested).await?;
            if read {
                let last = j == ops.len();
                self.read_bytes(w, &mut ops[i..j], !last).await?;
                end_requested = true;
            } else {
                for (k, op) in ops[i..j].iter().enumerate() {
                    if let Operation::Write(ww) = op {
                        self.write_bytes(w, ww, k == 0).await?;
                    }
                }
                end_requested = false;
            }
            started = true;
            i = j;
        }
        if started {
            self.stop(w, end_requested).await?;
        }
        Ok(())
    }
}

/// The ICSR2 flags that end a wait with an error.
const ERRORS: u32 = NACKF | AL | TMOF;

/// Whether any of `ccsr` (ICCSR) or `sr2` (ICSR2) is set, or one of
/// `errors` (ICSR2): `None` while neither.
fn check(ccsr: u32, sr2: u32, errors: u32) -> Option<Result<(), Error>> {
    let s = rd(ICSR2);
    let e = s & errors;
    if e & AL != 0 {
        return Some(Err(Error::ArbitrationLoss));
    }
    if e & TMOF != 0 {
        return Some(Err(Error::Timeout));
    }
    if e & NACKF != 0 {
        return Some(Err(Error::NoAcknowledge(NoAcknowledgeSource::Unknown)));
    }
    if rd(ICCSR) & ccsr != 0 || s & sr2 != 0 {
        return Some(Ok(()));
    }
    None
}

/// How a transfer waits for the RI3C's flags: [`Spin`] polls (the
/// blocking `I2c`), `Irq` sleeps until an interrupt (`AsyncI2c`). The
/// transfer itself is the same code.
trait Waiter {
    /// Waits for any of `ccsr` / `sr2`; any of `errors` ends it with that
    /// error.
    async fn until(&mut self, ccsr: u32, sr2: u32, errors: u32) -> Result<(), Error>;
}

/// The polling waiter.
struct Spin;

impl Waiter for Spin {
    async fn until(&mut self, ccsr: u32, sr2: u32, errors: u32) -> Result<(), Error> {
        loop {
            if let Some(r) = check(ccsr, sr2, errors) {
                return r;
            }
        }
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

/// An error on the address byte becomes NoAcknowledge(Address), on data
/// NoAcknowledge(Data).
fn no_ack(e: Error, address: bool) -> Error {
    match e {
        Error::NoAcknowledge(_) if address => Error::NoAcknowledge(NoAcknowledgeSource::Address),
        Error::NoAcknowledge(_) => Error::NoAcknowledge(NoAcknowledgeSource::Data),
        e => e,
    }
}

/// Selects PLL1P / the divider `code` as I3CCLK (manual 9.2: I3CCKSREQ,
/// wait for I3CCKSRDY, the divider and the source, I3CCKSREQ off, wait
/// for I3CCKSRDY to clear), with PRCR_S.PRC0 set; each wait at most
/// `spins` ([`SetupError::Timeout`], with I3CCKSREQ off and PRCR_S
/// restored).
fn set_i3cclk(code: u8, spins: u32) -> Result<(), SetupError> {
    let rdy = || unsafe { read_volatile(I3CCKCR) } & I3CCKSRDY != 0;
    critical_section::with(|_| unsafe {
        let prc = read_volatile(PRCR_S) & 0x00ff;
        write_volatile(PRCR_S, 0xa500 | prc | 1);
        let sel = read_volatile(I3CCKCR) & 0x0f;
        write_volatile(I3CCKCR, sel | I3CCKSREQ);
        let mut r = timeout::wait(spins, rdy);
        if r.is_ok() {
            write_volatile(I3CCKDIVCR, code);
            write_volatile(I3CCKCR, I3CCKSEL_PLL1P | I3CCKSREQ);
            write_volatile(I3CCKCR, I3CCKSEL_PLL1P);
            r = timeout::wait(spins, || !rdy());
        } else {
            write_volatile(I3CCKCR, sel);
        }
        write_volatile(PRCR_S, 0xa500 | prc);
        r.map_err(|_| SetupError::Timeout)
    })
}

impl ErrorType for I2c {
    type Error = Error;
}

impl I2cTrait<SevenBitAddress> for I2c {
    fn transaction(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        let r = block_on(self.run(&mut Spin, address, operations));
        if r.is_err() {
            self.recover();
        }
        r
    }
}


#[cfg(feature = "async")]
pub use asynch::{AsyncI2c, on_groupal1, on_ri3c0};

/// RI3C0 as an interrupt-driven embedded-hal-async `I2c` (feature
/// `async`): the same transfer as the blocking driver, sleeping where that
/// one polls.
///
/// A wait enables the interrupt of what it waits for: RDRF / TDRE through
/// ICCSIER (RXI0 / TXI0, routed to vectors 148 / 149 by SLIR148 / SLIR149,
/// FSP's `ICU_EVENT_RI3C0_RXI` / `_TXI` = 2B8h / 2B9h), START / STOP /
/// NACKF / AL / TMOF / TEND through ICSIER (EEI0 / TEI0, group interrupt
/// AL1, vector 105, GRPAL1_S bits 16 / 15). The handler turns them off
/// again and wakes the transfer, which looks at the flags. The short waits
/// for the RI3C to take a START or STOP request, for the bus to be free,
/// and the 10 SCL periods after an address alone still poll.
///
/// Register the handlers with [`bind_i2c_interrupts!`](crate::bind_i2c_interrupts)
/// and [`bind_group_al1!`](crate::bind_group_al1) (shared with the async
/// serial port). Dropping a transaction's future ends it as an error does:
/// a STOP, or the internal reset of the flags and requests.
#[cfg(feature = "async")]
mod asynch {
    use core::cell::RefCell;
    use core::future::poll_fn;
    use core::ptr::{read_volatile, write_volatile};
    use core::task::{Poll, Waker};

    use critical_section::Mutex;
    use embedded_hal::i2c::{ErrorType, Operation, SevenBitAddress};

    use super::{BASE, Error, I2c, Waiter, check, wr};
    use crate::icu::{Interrupt, Priority};

    const ICSIER: usize = BASE + 0x1d8;
    const ICCSIER: usize = BASE + 0x1e8;

    /// RXI0 / TXI0 on vectors 148 / 149.
    const RXI: Interrupt = unsafe { Interrupt::new(148, 148) };
    const TXI: Interrupt = unsafe { Interrupt::new(149, 149) };
    const SLIR148: *mut u16 = 0x8700_6928usize as _;
    const SLIR149: *mut u16 = 0x8700_692ausize as _;
    const EVENT_RXI: u16 = 0x2b8;
    const EVENT_TXI: u16 = 0x2b9;

    /// Group AL1: TEI0 is bit 15, EEI0 bit 16 (FSP's
    /// `ICU_EVENT_GROUP_RI3C0_TEI` / `_EEI`).
    const GROUP_AL1: Interrupt = unsafe { Interrupt::new(105, 105) };
    const GRPAL1: *const u32 = 0x8700_6744usize as _;
    const GENAL1: *mut u32 = 0x8700_67c4usize as _;
    const TEI_EEI: u32 = 1 << 15 | 1 << 16;

    static WAKER: Mutex<RefCell<Option<Waker>>> = Mutex::new(RefCell::new(None));

    fn quiet() {
        wr(ICSIER, 0);
        wr(ICCSIER, 0);
    }

    fn wake() {
        quiet();
        critical_section::with(|cs| {
            if let Some(w) = WAKER.borrow_ref_mut(cs).take() {
                w.wake();
            }
        });
    }

    /// The handler of RXI0 / TXI0 (vectors 148 / 149).
    pub extern "C" fn on_ri3c0() {
        wake();
    }

    /// This driver's part of the group AL1 handler
    /// ([`icu::on_group_al1`](crate::icu::on_group_al1)).
    pub fn on_groupal1() {
        if unsafe { read_volatile(GRPAL1) } & TEI_EEI != 0 {
            wake();
        }
    }

    /// The interrupt waiter.
    struct Irq;

    impl Waiter for Irq {
        async fn until(&mut self, ccsr: u32, sr2: u32, errors: u32) -> Result<(), Error> {
            poll_fn(|cx| {
                if let Some(r) = check(ccsr, sr2, errors) {
                    return Poll::Ready(r);
                }
                critical_section::with(|cs| {
                    *WAKER.borrow_ref_mut(cs) = Some(cx.waker().clone());
                });
                // The ICSIER bits sit where the ICSR2 flags do, the ICCSIER
                // bits where the ICCSR flags do.
                wr(ICSIER, sr2 | errors);
                wr(ICCSIER, ccsr);
                // It may have come before the enables.
                match check(ccsr, sr2, errors) {
                    Some(r) => {
                        quiet();
                        Poll::Ready(r)
                    }
                    None => Poll::Pending,
                }
            })
            .await
        }
    }

    /// The interrupt-driven I2C; see the [module](self).
    pub struct AsyncI2c {
        i2c: I2c,
    }

    impl I2c {
        /// Routes RXI0 / TXI0 to vectors 148 / 149, enables them and group
        /// AL1 with TEI0 / EEI0 at `priority` (group AL1's priority is
        /// shared with the async serial port's ERI12), and returns the
        /// interrupt-driven I2C.
        pub fn into_async(self, priority: Priority) -> AsyncI2c {
            quiet();
            critical_section::with(|_| unsafe {
                write_volatile(SLIR148, EVENT_RXI);
                write_volatile(SLIR149, EVENT_TXI);
                write_volatile(GENAL1, read_volatile(GENAL1) | TEI_EEI);
            });
            for i in [RXI, TXI] {
                i.clear_pending();
            }
            for i in [RXI, TXI, GROUP_AL1] {
                i.set_priority(priority);
                i.enable();
            }
            AsyncI2c { i2c: self }
        }
    }

    impl AsyncI2c {
        /// Disables its interrupts and gives back the blocking I2C.
        pub fn free(self) -> I2c {
            quiet();
            RXI.disable();
            TXI.disable();
            critical_section::with(|_| unsafe {
                write_volatile(GENAL1, read_volatile(GENAL1) & !TEI_EEI);
            });
            self.i2c
        }

        /// The SCL timing in use.
        pub fn timing(&self) -> super::Timing {
            self.i2c.timing()
        }
    }

    /// Ends a transaction cut short (its future dropped) as an error does.
    struct Recover<'a>(&'a mut I2c, bool);

    impl Drop for Recover<'_> {
        fn drop(&mut self) {
            if !self.1 {
                quiet();
                self.0.recover();
            }
        }
    }

    impl ErrorType for AsyncI2c {
        type Error = Error;
    }

    impl embedded_hal_async::i2c::I2c<SevenBitAddress> for AsyncI2c {
        async fn transaction(
            &mut self,
            address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Error> {
            let mut g = Recover(&mut self.i2c, false);
            let r = g.0.run(&mut Irq, address, operations).await;
            quiet();
            if r.is_err() {
                g.0.recover();
            }
            g.1 = true;
            r
        }
    }
}

/// Registers the async I2C's RXI0 / TXI0 handlers (vectors 148 / 149) with
/// rx-rt's `interrupt!`; its other interrupts come through group AL1: use
/// [`bind_group_al1!`](crate::bind_group_al1) too. Feature `async`.
#[cfg(feature = "async")]
#[macro_export]
macro_rules! bind_i2c_interrupts {
    () => {
        rx_rt::interrupt!(148, $crate::i2c::on_ri3c0);
        rx_rt::interrupt!(149, $crate::i2c::on_ri3c0);
    };
}
