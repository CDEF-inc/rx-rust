//! SPI master on RSPIA0, through the EK-RX74N's Arduino connector J13 (PA05
//! = RSPCK0-B = D13, PA06 = MOSI0-B = D11, PA07 = MISO0-B = D12), as a
//! blocking embedded-hal `SpiBus<u8>`.
//!
//! RSPIA is not the RX72M's RSPI: its control registers are 32 bits wide,
//! it has four-stage transmit and receive FIFOs, and its status flags
//! (SPTEF / SPRF) are set on a FIFO threshold and only cleared by a write
//! to SPSCLR (or by DTC / DMAC). The driver polls the FIFO levels instead
//! (SPTFSR.FREE, SPRFSR.FILL) and transfers one byte at a time: it reads
//! each byte before writing the next, so the receive FIFO cannot overrun
//! and there are no errors.
//!
//! The RSPIA runs in clock synchronous mode (SPCR.SPMS = 1, three wires):
//! the SSL pins stay I/O ports, so chip selects are GPIO pins of the
//! application. Its operating clock is PCLKA with the synchronizer bypassed
//! (SPCR.SYNDIS = 1; the other choice, SPICLK, is not set up by
//! [`crate::clock`]). One command (SPCMD0), 8-bit frames, MSB first, SPDR
//! accessed by byte. The default delays apply: RSPCK delay and SSL
//! negation delay 1 RSPCK each, next access delay 1 RSPCK + 5 PCLKA.
//!
//! The bit rate is PCLKA / (2 * (SPBR + 1) * 2^BRDV): the highest one not
//! above the requested rate.
//!
//! Registers at RSPIA0's secure addresses (0x8745C000), addressed
//! directly.

use core::convert::Infallible;
use core::ptr::{read_volatile, write_volatile};

use embedded_hal::spi::{ErrorType, Mode, Phase, Polarity, SpiBus};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::mstp::{self, Mstpcr};
use crate::pac;

const BASE: usize = 0x8745_c000;
const SPDR: *mut u8 = BASE as _;
const SPCR: *mut u32 = (BASE + 0x08) as _;
const SPPCR: *mut u8 = (BASE + 0x0e) as _;
const SPBR: *mut u8 = (BASE + 0x11) as _;
const SPCMD0: *mut u32 = (BASE + 0x14) as _;
const SPDCR: *mut u16 = (BASE + 0x40) as _;
const SPFCR: *mut u16 = (BASE + 0x44) as _;
const SPSR: *const u16 = (BASE + 0x52) as _;
const SPTFSR: *const u8 = (BASE + 0x58) as _;
const SPRFSR: *const u8 = (BASE + 0x5c) as _;
const SPFCLR: *mut u8 = (BASE + 0x6c) as _;

// SPCR bits
const SPE: u32 = 1 << 0;
const SPMS: u32 = 1 << 24;
const MSTR: u32 = 1 << 30;
const SYNDIS: u32 = 1 << 31;
// SPSR.IDLNF
const IDLNF: u16 = 1 << 9;
// SPCMD0.SPB for 8 bits (the length minus 1)
const SPB_8: u32 = 0b00111 << 16;

/// PSEL of the RSPIA0-B pins (RSPCK0-B, MOSI0-B, MISO0-B, SSL0n-B).
const PSEL_RSPI: u8 = 0b001_0000;

/// The byte sent while reading.
const READ_FILL: u8 = 0xff;

/// The bit rate settings: SPBR and SPCMDm.BRDV.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitRate {
    pub spbr: u8,
    pub brdv: u8,
    /// The resulting rate in bits per second.
    pub hz: u32,
}

impl BitRate {
    /// The highest rate not above `hz` for the operating clock `tclk`:
    /// rate = tclk / (2 * (spbr + 1) * 2^brdv). The smallest BRDV that fits
    /// gives it. `None` if `hz` is below tclk / 4096 (SPBR = 255, BRDV =
    /// 3).
    pub fn new(tclk: u32, hz: u32) -> Option<Self> {
        if hz == 0 {
            return None;
        }
        for brdv in 0..4u8 {
            let div = 2u64 << brdv;
            let n = (tclk as u64).div_ceil(div * hz as u64).max(1);
            if n <= 256 {
                let spbr = (n - 1) as u8;
                return Some(BitRate { spbr, brdv, hz: (tclk as u64 / (div * n)) as u32 });
            }
        }
        None
    }
}

/// RSPIA0 as an SPI master.
pub struct Spi {
    rspi: pac::Rspia0,
    rate: BitRate,
    sck: Pin<0x0a, 5, Function>,
    mosi: Pin<0x0a, 6, Function>,
    miso: Pin<0x0a, 7, Function>,
}

impl Spi {
    /// Sets up RSPIA0 (taking it out of module stop, MSTPCRB.MSTPB19) as a
    /// master on PA05 / PA06 / PA07 in SPI `mode` at the highest bit rate
    /// not above `hz` of PCLKA.
    ///
    /// Panics if `hz` is below PCLKA / 4096.
    pub fn rspia0<CM, OM, IM>(
        rspi: pac::Rspia0,
        sck: Pin<0x0a, 5, CM>,
        mosi: Pin<0x0a, 6, OM>,
        miso: Pin<0x0a, 7, IM>,
        mode: Mode,
        hz: u32,
        clocks: &Clocks,
    ) -> Self {
        let rate = BitRate::new(clocks.pclka, hz).expect("bit rate out of range");
        mstp::start(Mstpcr::B, 19);
        unsafe { write_volatile(SPCR, 0) };
        let sck = sck.into_function(PSEL_RSPI);
        let mosi = mosi.into_function(PSEL_RSPI);
        // PCR stays set in the function mode: an unconnected MISO reads 1.
        let miso = miso.into_pull_up_input().into_function(PSEL_RSPI);
        unsafe {
            // Normal mode (no loopback), MOSI keeps the last bit when idle.
            write_volatile(SPPCR, 0);
            write_volatile(SPBR, rate.spbr);
            // 8 bits, MSB first, SSLA = 0, no burst, default delays.
            write_volatile(
                SPCMD0,
                SPB_8
                    | (rate.brdv as u32) << 2
                    | ((mode.polarity == Polarity::IdleHigh) as u32) << 1
                    | (mode.phase == Phase::CaptureOnSecondTransition) as u32,
            );
            write_volatile(SPDCR, 0);
            write_volatile(SPFCR, 0);
            write_volatile(SPFCLR, 1);
            // Clock synchronous master on PCLKA, transmit-receive, then
            // enabled.
            write_volatile(SPCR, SYNDIS | MSTR | SPMS);
            write_volatile(SPCR, SYNDIS | MSTR | SPMS | SPE);
        }
        Spi { rspi, rate, sck, mosi, miso }
    }

    /// The bit rate settings in use.
    pub fn bit_rate(&self) -> BitRate {
        self.rate
    }

    /// Waits for the last transfer, disables the RSPIA and gives it and its
    /// pins back (still in the RSPIA function, for `rspia0` again).
    #[allow(clippy::type_complexity)]
    pub fn free(
        self,
    ) -> (pac::Rspia0, Pin<0x0a, 5, Function>, Pin<0x0a, 6, Function>, Pin<0x0a, 7, Function>) {
        self.flush_idle();
        unsafe { write_volatile(SPCR, 0) };
        (self.rspi, self.sck, self.mosi, self.miso)
    }

    /// Connects the transmit side of the shift register to its receive side
    /// (SPPCR.SPLP2 = 1, `inverted` = false) or through an inverter
    /// (SPLP = 1), or back to MISO (`None`). A self-test of the RSPIA
    /// without wiring; RSPCK and MOSI still go out on their pins. The RSPIA
    /// is disabled while SPPCR changes.
    pub fn set_loopback(&mut self, loopback: Option<bool>) {
        self.flush_idle();
        unsafe {
            let spcr = read_volatile(SPCR);
            write_volatile(SPCR, spcr & !SPE);
            write_volatile(
                SPPCR,
                match loopback {
                    None => 0,
                    Some(false) => 1 << 1,
                    Some(true) => 1 << 0,
                },
            );
            write_volatile(SPCR, spcr | SPE);
        }
    }

    /// Sends `b` and returns the byte received at the same time.
    fn transfer_byte(&mut self, b: u8) -> u8 {
        unsafe {
            while read_volatile(SPTFSR) & 0b111 == 0 {}
            write_volatile(SPDR, b);
            while read_volatile(SPRFSR) & 0b111 == 0 {}
            read_volatile(SPDR)
        }
    }

    /// Waits until the RSPIA is idle (SPSR.IDLNF = 0): the transmit FIFO
    /// empty and the last frame and its delays over.
    fn flush_idle(&self) {
        while unsafe { read_volatile(SPSR) } & IDLNF != 0 {}
    }
}

impl ErrorType for Spi {
    type Error = Infallible;
}

impl SpiBus<u8> for Spi {
    /// Reads `words`, sending FFh for each.
    fn read(&mut self, words: &mut [u8]) -> Result<(), Infallible> {
        for w in words {
            *w = self.transfer_byte(READ_FILL);
        }
        Ok(())
    }

    fn write(&mut self, words: &[u8]) -> Result<(), Infallible> {
        for &w in words {
            self.transfer_byte(w);
        }
        Ok(())
    }

    /// Sends `write` and fills `read`; the longer one sets the length, with
    /// FFh sent past the end of `write` and the bytes past the end of
    /// `read` dropped.
    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Infallible> {
        for i in 0..read.len().max(write.len()) {
            let r = self.transfer_byte(write.get(i).copied().unwrap_or(READ_FILL));
            if let Some(slot) = read.get_mut(i) {
                *slot = r;
            }
        }
        Ok(())
    }

    fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Infallible> {
        for w in words {
            *w = self.transfer_byte(*w);
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Infallible> {
        self.flush_idle();
        Ok(())
    }
}
