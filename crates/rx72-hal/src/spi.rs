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
//! cannot overrun. The waits are bounded: a byte that does not finish
//! within four byte times gives [`Error::Timeout`].

use core::fmt;
use core::ptr::{read_volatile, write_volatile};

use embedded_hal::spi::{ErrorKind, ErrorType, Mode, Phase, Polarity, SpiBus};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::mstp::{self, Mstpcr};
use crate::pac;
use crate::timeout::{self, OutOfRange};

/// An SPI transfer that did not finish in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// SPTEF, SPRF or IDLNF did not change within four byte times.
    Timeout,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SPI timeout")
    }
}

impl core::error::Error for Error {}

impl embedded_hal::spi::Error for Error {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

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
    /// The bound of a wait: four byte times (about 11 RSPCK each).
    spins: u32,
    sck: Pin<0x05, 6, Function>,
    mosi: Pin<0x05, 4, Function>,
    miso: Pin<0x05, 5, Function>,
}

impl Spi {
    /// Sets up RSPI2 (taking it out of module stop) as a master on P56 /
    /// P54 / P55 in SPI `mode` at the highest bit rate not above `hz`.
    ///
    /// Panics if `hz` is below PCLKA / 4096 (see
    /// [`try_rspi2`](Self::try_rspi2)).
    pub fn rspi2<CM, OM, IM>(
        rspi: pac::Rspi2,
        sck: Pin<0x05, 6, CM>,
        mosi: Pin<0x05, 4, OM>,
        miso: Pin<0x05, 5, IM>,
        mode: Mode,
        hz: u32,
        clocks: &Clocks,
    ) -> Self {
        Self::try_rspi2(rspi, sck, mosi, miso, mode, hz, clocks).expect("bit rate out of range")
    }

    /// [`rspi2`](Self::rspi2), or [`OutOfRange`] (the peripheral and pins
    /// untouched, and dropped) if `hz` is below PCLKA / 4096.
    pub fn try_rspi2<CM, OM, IM>(
        rspi: pac::Rspi2,
        sck: Pin<0x05, 6, CM>,
        mosi: Pin<0x05, 4, OM>,
        miso: Pin<0x05, 5, IM>,
        mode: Mode,
        hz: u32,
        clocks: &Clocks,
    ) -> Result<Self, OutOfRange> {
        let rate = BitRate::new(clocks.pclka, hz).ok_or(OutOfRange)?;
        let spins = timeout::spins(clocks.iclk, 4 * 11_000_000 / rate.hz + 1);
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
        Ok(Spi { rspi, rate, spins, sck, mosi, miso })
    }

    /// The bit rate settings in use.
    pub fn bit_rate(&self) -> BitRate {
        self.rate
    }

    /// Waits for the last transfer (at most four byte times), disables the
    /// RSPI and gives it and its pins back (still in the RSPI function, for
    /// `rspi2` again).
    #[allow(clippy::type_complexity)]
    pub fn free(
        self,
    ) -> (pac::Rspi2, Pin<0x05, 6, Function>, Pin<0x05, 4, Function>, Pin<0x05, 5, Function>) {
        let _ = self.flush_idle();
        self.rspi.spcr().write(|w| unsafe { w.bits(0) });
        (self.rspi, self.sck, self.mosi, self.miso)
    }

    /// Connects the transmit side of the shift register to its receive side
    /// (SPPCR.SPLP2 = 1, `inverted` = false) or through an inverter
    /// (SPLP = 1), or back to MISO (`None`). A self-test of the RSPI
    /// without wiring; RSPCK and MOSI still go out on their pins. The RSPI
    /// is disabled while SPPCR changes, after the last transfer
    /// ([`Error::Timeout`] if that does not end).
    pub fn set_loopback(&mut self, loopback: Option<bool>) -> Result<(), Error> {
        self.flush_idle()?;
        self.rspi.spcr().modify(|_, w| w.spe().clear_bit());
        self.rspi.sppcr().write(|w| match loopback {
            None => w,
            Some(false) => w.splp2().set_bit(),
            Some(true) => w.splp().set_bit(),
        });
        self.rspi.spcr().modify(|_, w| w.spe().set_bit());
        Ok(())
    }

    /// Sends `b` and returns the byte received at the same time.
    fn transfer_byte(&mut self, b: u8) -> Result<u8, Error> {
        let spsr = || self.rspi.spsr().read();
        timeout::wait(self.spins, || spsr().sptef().bit_is_set()).map_err(|_| Error::Timeout)?;
        unsafe { write_volatile(SPDR_RSPI2, b) };
        timeout::wait(self.spins, || spsr().sprf().bit_is_set()).map_err(|_| Error::Timeout)?;
        Ok(unsafe { read_volatile(SPDR_RSPI2) })
    }

    /// Waits until the RSPI is idle (SPSR.IDLNF = 0): the transmit buffer
    /// empty and the last frame and its delays over.
    fn flush_idle(&self) -> Result<(), Error> {
        timeout::wait(self.spins, || self.rspi.spsr().read().idlnf().bit_is_clear())
            .map_err(|_| Error::Timeout)
    }
}

impl ErrorType for Spi {
    type Error = Error;
}

impl SpiBus<u8> for Spi {
    /// Reads `words`, sending FFh for each.
    fn read(&mut self, words: &mut [u8]) -> Result<(), Error> {
        for w in words {
            *w = self.transfer_byte(READ_FILL)?;
        }
        Ok(())
    }

    fn write(&mut self, words: &[u8]) -> Result<(), Error> {
        for &w in words {
            self.transfer_byte(w)?;
        }
        Ok(())
    }

    /// Sends `write` and fills `read`; the longer one sets the length, with
    /// FFh sent past the end of `write` and the bytes past the end of
    /// `read` dropped.
    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Error> {
        for i in 0..read.len().max(write.len()) {
            let r = self.transfer_byte(write.get(i).copied().unwrap_or(READ_FILL))?;
            if let Some(slot) = read.get_mut(i) {
                *slot = r;
            }
        }
        Ok(())
    }

    fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Error> {
        for w in words {
            *w = self.transfer_byte(*w)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.flush_idle()
    }
}

/// The data register the DMA channels use (the byte access).
#[cfg(feature = "async")]
const SPDR: *mut u8 = SPDR_RSPI2;
/// SPRI2 (receive buffer full) / SPTI2 (transmit buffer empty).
#[cfg(feature = "async")]
const RXI: crate::icu::Interrupt = unsafe { crate::icu::Interrupt::new(108, 108) };
#[cfg(feature = "async")]
const TXI: crate::icu::Interrupt = unsafe { crate::icu::Interrupt::new(109, 109) };

/// The requests have fixed vectors here.
#[cfg(feature = "async")]
fn route_requests() {}

#[cfg(feature = "async")]
impl Spi {
    /// Drops a received byte left in SPDR and clears an overrun (SPSR.OVRF,
    /// cleared by writing 0 after reading 1).
    fn discard_received(&self) {
        if self.rspi.spsr().read().sprf().bit_is_set() {
            let _ = unsafe { read_volatile(SPDR_RSPI2) };
        }
        if self.rspi.spsr().read().ovrf().bit_is_set() {
            self.rspi.spsr().modify(|_, w| w.ovrf().clear_bit());
        }
    }

    /// SPCR.SPTIE / SPRIE on or off. SPE is set together with SPTIE, which
    /// gives the first SPTI2 while the transmit buffer is empty.
    fn irq_requests(&self, on: bool) {
        let spcr = self.rspi.spcr();
        if on {
            spcr.modify(|_, w| w.spe().clear_bit());
            spcr.modify(|_, w| w.sptie().set_bit().sprie().set_bit().spe().set_bit());
        } else {
            spcr.modify(|_, w| w.sptie().clear_bit().sprie().clear_bit());
        }
    }
}

#[cfg(feature = "async")]
pub use asynch::AsyncSpi;

/// RSPI2 with two DMA channels, as an embedded-hal-async `SpiBus<u8>`
/// (feature `async`).
///
/// SPTI2 (transmit buffer empty, vector 109) paces the transmit channel,
/// which writes SPDR; SPRI2 (receive buffer full, vector 108) paces the
/// receive channel, which reads it. Both sources stay at priority 0, so
/// that they only activate the DMAC (the CPU never takes them, also after
/// the count). A transfer ends with the receive channel's end interrupt:
/// enable it with [`dma::enable_interrupts`](crate::dma::enable_interrupts)
/// and [`bind_dma_interrupts!`](crate::bind_dma_interrupts).
///
/// The bytes past the shorter buffer of `transfer`, and every byte of
/// `read`, are sent as FFh; received bytes nobody asked for go to a sink.
/// A transfer has no time bound: wrap it in a timeout (embassy-time's
/// `with_timeout`) where the bus can stall. Dropping its future stops both
/// channels.
///
/// **Do not leak a transfer's future** (`core::mem::forget`, a reference
/// cycle): its buffers' borrows then end while the DMAC still reads and
/// writes them, and no drop guard runs to stop it. `SpiBus`'s methods are
/// safe by the trait, so this is a documented requirement, as in the
/// other DMA-based async HALs (see [`Channel::copy_async`](crate::dma::Channel::copy_async),
/// which is unsafe for the same reason).
#[cfg(feature = "async")]
mod asynch {
    use core::ptr::addr_of_mut;

    use embedded_hal::spi::ErrorType;

    use super::{Error, READ_FILL, SPDR, Spi};
    use crate::dma::{Channel, Size, Step, Transfer, Trigger};

    /// The byte sent where there is nothing to send.
    static FILL: u8 = READ_FILL;
    /// Where the bytes nobody reads go.
    static mut SINK: u8 = 0;

    /// The units of one DMA transfer at most.
    const CHUNK: usize = 0xffff;

    /// The SPI with its transmit channel `T` and receive channel `R`; see
    /// the [module](self).
    pub struct AsyncSpi<const T: u8, const R: u8> {
        spi: Spi,
        tx: Channel<T>,
        rx: Channel<R>,
    }

    impl Spi {
        /// The SPI driven by DMA channels `tx` and `rx` (two different
        /// ones); see [`AsyncSpi`].
        pub fn into_async<const T: u8, const R: u8>(
            self,
            tx: Channel<T>,
            rx: Channel<R>,
        ) -> AsyncSpi<T, R> {
            const { assert!(T != R, "the transmit and receive channels must differ") };
            super::route_requests();
            for i in [super::RXI, super::TXI] {
                i.disable();
                i.clear_priority();
                i.clear_pending();
            }
            AsyncSpi { spi: self, tx, rx }
        }
    }

    /// Stops a transfer that did not end (its future dropped).
    struct Stop<'a, const T: u8, const R: u8>(&'a mut AsyncSpi<T, R>);

    impl<const T: u8, const R: u8> Drop for Stop<'_, T, R> {
        fn drop(&mut self) {
            self.0.end();
        }
    }

    impl<const T: u8, const R: u8> AsyncSpi<T, R> {
        /// Ends the DMA use and gives back the blocking SPI and the
        /// channels.
        pub fn free(mut self) -> (Spi, Channel<T>, Channel<R>) {
            self.end();
            (self.spi, self.tx, self.rx)
        }

        /// The bit rate settings in use.
        pub fn bit_rate(&self) -> super::BitRate {
            self.spi.bit_rate()
        }

        /// Interrupt requests off, channels stopped. A transfer cut short
        /// (its future dropped) can leave bytes in the transmit buffer and
        /// the shift register: they are let finish (bounded) and what they
        /// brought into the receive side is discarded, so that the next
        /// transfer, async or blocking, does not read an old response.
        fn end(&mut self) {
            self.spi.irq_requests(false);
            self.tx.stop();
            self.rx.stop();
            let _ = self.spi.flush_idle();
            self.spi.discard_received();
            for i in [super::RXI, super::TXI] {
                i.disable();
                i.clear_pending();
            }
        }

        /// `n` (1 to 65535) frames: sent from `write` (or FFh), received
        /// into `read` (or the sink).
        ///
        /// # Safety
        ///
        /// `read` / `write` must have `n` bytes for the whole transfer.
        async unsafe fn run(&mut self, read: Option<*mut u8>, write: Option<*const u8>, n: usize) {
            let (dst, dst_step) = match read {
                Some(p) => (p as usize, Step::Increment),
                None => (addr_of_mut!(SINK) as usize, Step::Fixed),
            };
            let (src, src_step) = match write {
                Some(p) => (p as usize, Step::Increment),
                None => (&FILL as *const u8 as usize, Step::Fixed),
            };
            let spdr = SPDR as usize;
            let count = n as u16;
            unsafe {
                self.rx.start(&Transfer {
                    src: spdr,
                    src_step: Step::Fixed,
                    dst,
                    dst_step,
                    size: Size::U8,
                    count,
                    trigger: Trigger::Interrupt(super::RXI),
                });
                self.tx.start(&Transfer {
                    src,
                    src_step,
                    dst: spdr,
                    dst_step: Step::Fixed,
                    size: Size::U8,
                    count,
                    trigger: Trigger::Interrupt(super::TXI),
                });
            }
            for i in [super::RXI, super::TXI] {
                i.clear_pending();
                i.enable();
            }
            // The transmit buffer is empty: enabling the request gives the
            // first one.
            self.spi.irq_requests(true);
            let stop = Stop(self);
            stop.0.rx.done().await;
            drop(stop);
        }

        /// [`run`](Self::run) over any length, in chunks.
        async unsafe fn run_all(&mut self, read: Option<*mut u8>, write: Option<*const u8>, n: usize) {
            let mut done = 0;
            while done < n {
                let k = (n - done).min(CHUNK);
                unsafe {
                    self.run(read.map(|p| p.add(done)), write.map(|p| p.add(done)), k).await;
                }
                done += k;
            }
        }
    }

    impl<const T: u8, const R: u8> ErrorType for AsyncSpi<T, R> {
        type Error = Error;
    }

    impl<const T: u8, const R: u8> embedded_hal_async::spi::SpiBus<u8> for AsyncSpi<T, R> {
        /// Reads `words`, sending FFh for each.
        async fn read(&mut self, words: &mut [u8]) -> Result<(), Error> {
            unsafe { self.run_all(Some(words.as_mut_ptr()), None, words.len()).await };
            Ok(())
        }

        async fn write(&mut self, words: &[u8]) -> Result<(), Error> {
            unsafe { self.run_all(None, Some(words.as_ptr()), words.len()).await };
            Ok(())
        }

        /// Sends `write` and fills `read`; the longer one sets the length,
        /// with FFh sent past the end of `write` and the bytes past the end
        /// of `read` dropped.
        async fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Error> {
            let common = read.len().min(write.len());
            unsafe {
                self.run_all(Some(read.as_mut_ptr()), Some(write.as_ptr()), common).await;
                let r = &mut read[common..];
                self.run_all(Some(r.as_mut_ptr()), None, r.len()).await;
                let w = &write[common..];
                self.run_all(None, Some(w.as_ptr()), w.len()).await;
            }
            Ok(())
        }

        /// The transmit channel reads each byte before the receive channel
        /// writes it back: the transmit side is always ahead.
        async fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Error> {
            let p = words.as_mut_ptr();
            unsafe { self.run_all(Some(p), Some(p as *const u8), words.len()).await };
            Ok(())
        }

        /// Waits (bounded, as the blocking `flush`) until the SPI is idle.
        async fn flush(&mut self) -> Result<(), Error> {
            self.spi.flush_idle()
        }
    }
}
