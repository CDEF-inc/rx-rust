//! SPI master on RSPI2, through the AP-RX72M-0A's wireless module connector
//! CN9 (P56 = RSPCKC, P54 = MOSIC, P55 = MISOC), as a blocking
//! embedded-hal `SpiBus<u8>`.
//!
//! The RSPI runs in clock synchronous mode (SPCR.SPMS = 1, three wires):
//! the SSL pins stay I/O ports, so chip selects are GPIO pins of the
//! application (embedded-hal-bus's `ExclusiveDevice`, for example). One
//! command (SPCMD0), one 8-bit frame per transfer, MSB first, with the
//! SPDR byte access (SPDCR.SPBYT = 1). The default delays apply: RSPCK
//! delay and SSL negation delay 1 RSPCK each, next access delay 1 RSPCK +
//! 2 PCLKA, about 11 RSPCK per byte.
//!
//! The bit rate is PCLKA / (2 * (SPBR + 1) * 2^BRDV): the highest one not
//! above the requested rate (PCLKA 120 MHz: 30 MHz is SPBR = 1, 1 MHz is
//! SPBR = 59).
//!
//! The driver polls SPSR.SPTEF / SPRF, which the manual allows with
//! SPCR.SPTIE / SPRIE cleared (45.5.4), and transfers one byte at a time:
//! it reads each byte before writing the next, so the receive buffer
//! cannot overrun and there are no errors.

use core::convert::Infallible;
use core::ptr::{read_volatile, write_volatile};

use embedded_hal::spi::{ErrorType, Mode, Phase, Polarity, SpiBus};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::mstp::{self, Mstpcr};
use crate::pac;

/// PFS.PSEL of the RSPI functions (RSPCKx, MOSIx, MISOx, SSLxn).
const PSEL_RSPI: u8 = 0b001101;

/// SPDR.HH, the SPDR byte for the byte access (SPDCR.SPBYT = 1).
const SPDR_RSPI2: *mut u8 = 0x000d_0304 as _;

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
    /// The highest rate not above `hz` for the peripheral clock `pclk`:
    /// rate = pclk / (2 * (spbr + 1) * 2^brdv). The smallest BRDV that fits
    /// gives it (a larger BRDV only makes the steps coarser). `None` if
    /// `hz` is below pclk / 4096 (SPBR = 255, BRDV = 3).
    pub fn new(pclk: u32, hz: u32) -> Option<Self> {
        if hz == 0 {
            return None;
        }
        for brdv in 0..4u8 {
            let div = 2u64 << brdv;
            let n = (pclk as u64).div_ceil(div * hz as u64).max(1);
            if n <= 256 {
                let spbr = (n - 1) as u8;
                return Some(BitRate { spbr, brdv, hz: (pclk as u64 / (div * n)) as u32 });
            }
        }
        None
    }
}

/// RSPI2 as an SPI master.
pub struct Spi {
    rspi: pac::Rspi2,
    rate: BitRate,
    sck: Pin<0x05, 6, Function>,
    mosi: Pin<0x05, 4, Function>,
    miso: Pin<0x05, 5, Function>,
}

impl Spi {
    /// Sets up RSPI2 (taking it out of module stop) as a master on P56 /
    /// P54 / P55 in SPI `mode` at the highest bit rate not above `hz`.
    ///
    /// Panics if `hz` is below PCLKA / 4096.
    pub fn rspi2<CM, OM, IM>(
        rspi: pac::Rspi2,
        sck: Pin<0x05, 6, CM>,
        mosi: Pin<0x05, 4, OM>,
        miso: Pin<0x05, 5, IM>,
        mode: Mode,
        hz: u32,
        clocks: &Clocks,
    ) -> Self {
        let rate = BitRate::new(clocks.pclka, hz).expect("bit rate out of range");
        mstp::start(Mstpcr::C, 22);
        rspi.spcr().write(|w| unsafe { w.bits(0) });
        let sck = sck.into_function(PSEL_RSPI);
        let mosi = mosi.into_function(PSEL_RSPI);
        // PCR stays set in the function mode: an unconnected MISO reads 1.
        let miso = miso.into_pull_up_input().into_function(PSEL_RSPI);
        // Normal mode (no loopback), MOSI keeps the last bit when idle.
        rspi.sppcr().write(|w| unsafe { w.bits(0) });
        rspi.sslp().write(|w| unsafe { w.bits(0) });
        rspi.spbr().write(|w| unsafe { w.bits(rate.spbr) });
        // One frame, byte access, SPDR reads the receive buffer.
        rspi.spdcr().write(|w| w.spbyt().set_bit());
        rspi.spckd().write(|w| unsafe { w.bits(0) });
        rspi.sslnd().write(|w| unsafe { w.bits(0) });
        rspi.spnd().write(|w| unsafe { w.bits(0) });
        // No parity, no idle interrupt, RSPCK auto-stop off.
        rspi.spcr2().write(|w| unsafe { w.bits(0) });
        // A sequence of SPCMD0 only.
        rspi.spscr().write(|w| unsafe { w.bits(0) });
        // 8 bits (SPB = 0100b), MSB first, SSLA = 0, no burst, default delays.
        rspi.spcmd0().write(|w| unsafe {
            w.cpha()
                .bit(mode.phase == Phase::CaptureOnSecondTransition)
                .cpol()
                .bit(mode.polarity == Polarity::IdleHigh)
                .brdv()
                .bits(rate.brdv)
                .spb()
                .bits(0b0100)
        });
        // Clock synchronous master, full duplex, then enabled.
        rspi.spcr().write(|w| w.spms().set_bit().mstr().set_bit());
        rspi.spcr().modify(|_, w| w.spe().set_bit());
        Spi { rspi, rate, sck, mosi, miso }
    }

    /// The bit rate settings in use.
    pub fn bit_rate(&self) -> BitRate {
        self.rate
    }

    /// Waits for the last transfer, disables the RSPI and gives it and its
    /// pins back (still in the RSPI function, for `rspi2` again).
    #[allow(clippy::type_complexity)]
    pub fn free(
        self,
    ) -> (pac::Rspi2, Pin<0x05, 6, Function>, Pin<0x05, 4, Function>, Pin<0x05, 5, Function>) {
        self.flush_idle();
        self.rspi.spcr().write(|w| unsafe { w.bits(0) });
        (self.rspi, self.sck, self.mosi, self.miso)
    }

    /// Connects the transmit side of the shift register to its receive side
    /// (SPPCR.SPLP2 = 1, `inverted` = false) or through an inverter
    /// (SPLP = 1), or back to MISO (`None`). A self-test of the RSPI
    /// without wiring; RSPCK and MOSI still go out on their pins. The RSPI
    /// is disabled while SPPCR changes.
    pub fn set_loopback(&mut self, loopback: Option<bool>) {
        self.flush_idle();
        self.rspi.spcr().modify(|_, w| w.spe().clear_bit());
        self.rspi.sppcr().write(|w| match loopback {
            None => w,
            Some(false) => w.splp2().set_bit(),
            Some(true) => w.splp().set_bit(),
        });
        self.rspi.spcr().modify(|_, w| w.spe().set_bit());
    }

    /// Sends `b` and returns the byte received at the same time.
    fn transfer_byte(&mut self, b: u8) -> u8 {
        while self.rspi.spsr().read().sptef().bit_is_clear() {}
        unsafe { write_volatile(SPDR_RSPI2, b) };
        while self.rspi.spsr().read().sprf().bit_is_clear() {}
        unsafe { read_volatile(SPDR_RSPI2) }
    }

    /// Waits until the RSPI is idle (SPSR.IDLNF = 0): the transmit buffer
    /// empty and the last frame and its delays over.
    fn flush_idle(&self) {
        while self.rspi.spsr().read().idlnf().bit_is_set() {}
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
