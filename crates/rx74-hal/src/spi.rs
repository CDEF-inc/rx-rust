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

use core::fmt;
use core::ptr::{read_volatile, write_volatile};

use embedded_hal::spi::{ErrorKind, ErrorType, Mode, Phase, Polarity, SpiBus};

use crate::clock::Clocks;
use crate::gpio::{Function, Pin};
use crate::mstp::{self, Mstpcr};
use crate::pac;
use crate::timeout::{self, OutOfRange};

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

/// An SPI transfer that did not finish in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The FIFOs or IDLNF did not change within four byte times.
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

/// RSPIA0 as an SPI master.
pub struct Spi {
    rspi: pac::Rspia0,
    rate: BitRate,
    /// The bound of a wait: four byte times (about 11 RSPCK each).
    spins: u32,
    sck: Pin<0x0a, 5, Function>,
    mosi: Pin<0x0a, 6, Function>,
    miso: Pin<0x0a, 7, Function>,
}

impl Spi {
    /// Sets up RSPIA0 (taking it out of module stop, MSTPCRB.MSTPB19) as a
    /// master on PA05 / PA06 / PA07 in SPI `mode` at the highest bit rate
    /// not above `hz` of PCLKA.
    ///
    /// Panics if `hz` is below PCLKA / 4096 (see
    /// [`try_rspia0`](Self::try_rspia0)).
    pub fn rspia0<CM, OM, IM>(
        rspi: pac::Rspia0,
        sck: Pin<0x0a, 5, CM>,
        mosi: Pin<0x0a, 6, OM>,
        miso: Pin<0x0a, 7, IM>,
        mode: Mode,
        hz: u32,
        clocks: &Clocks,
    ) -> Self {
        Self::try_rspia0(rspi, sck, mosi, miso, mode, hz, clocks).expect("bit rate out of range")
    }

    /// [`rspia0`](Self::rspia0), or [`OutOfRange`] (the peripheral and pins
    /// untouched, and dropped) if `hz` is below PCLKA / 4096.
    pub fn try_rspia0<CM, OM, IM>(
        rspi: pac::Rspia0,
        sck: Pin<0x0a, 5, CM>,
        mosi: Pin<0x0a, 6, OM>,
        miso: Pin<0x0a, 7, IM>,
        mode: Mode,
        hz: u32,
        clocks: &Clocks,
    ) -> Result<Self, OutOfRange> {
        let rate = BitRate::new(clocks.pclka, hz).ok_or(OutOfRange)?;
        let spins = timeout::spins(clocks.cpuclk, 4 * 11_000_000 / rate.hz + 1);
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
        Ok(Spi { rspi, rate, spins, sck, mosi, miso })
    }

    /// The bit rate settings in use.
    pub fn bit_rate(&self) -> BitRate {
        self.rate
    }

    /// Waits for the last transfer (at most four byte times), disables the
    /// RSPIA and gives it and its pins back (still in the RSPIA function,
    /// for `rspia0` again).
    #[allow(clippy::type_complexity)]
    pub fn free(
        self,
    ) -> (pac::Rspia0, Pin<0x0a, 5, Function>, Pin<0x0a, 6, Function>, Pin<0x0a, 7, Function>) {
        let _ = self.flush_idle();
        unsafe { write_volatile(SPCR, 0) };
        (self.rspi, self.sck, self.mosi, self.miso)
    }

    /// Connects the transmit side of the shift register to its receive side
    /// (SPPCR.SPLP2 = 1, `inverted` = false) or through an inverter
    /// (SPLP = 1), or back to MISO (`None`). A self-test of the RSPIA
    /// without wiring; RSPCK and MOSI still go out on their pins. The RSPIA
    /// is disabled while SPPCR changes, after the last transfer
    /// ([`Error::Timeout`] if that does not end).
    pub fn set_loopback(&mut self, loopback: Option<bool>) -> Result<(), Error> {
        self.flush_idle()?;
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
        Ok(())
    }

    /// Sends `b` and returns the byte received at the same time.
    fn transfer_byte(&mut self, b: u8) -> Result<u8, Error> {
        let ready = |r: *const u8| unsafe { read_volatile(r) } & 0b111 != 0;
        timeout::wait(self.spins, || ready(SPTFSR)).map_err(|_| Error::Timeout)?;
        unsafe { write_volatile(SPDR, b) };
        timeout::wait(self.spins, || ready(SPRFSR)).map_err(|_| Error::Timeout)?;
        Ok(unsafe { read_volatile(SPDR) })
    }

    /// Waits until the RSPIA is idle (SPSR.IDLNF = 0): the transmit FIFO
    /// empty and the last frame and its delays over.
    fn flush_idle(&self) -> Result<(), Error> {
        timeout::wait(self.spins, || unsafe { read_volatile(SPSR) } & IDLNF == 0)
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

/// SPRI0 (receive) / SPTI0 (transmit) are selectable interrupts: routed to
/// vectors 146 / 147 (SLIR146 / SLIR147 = their event numbers, FSP's
/// `ICU_EVENT_RSPI0_SPRI` / `_SPTI`).
#[cfg(feature = "async")]
const RXI: crate::icu::Interrupt = unsafe { crate::icu::Interrupt::new(146, 146) };
#[cfg(feature = "async")]
const TXI: crate::icu::Interrupt = unsafe { crate::icu::Interrupt::new(147, 147) };

#[cfg(feature = "async")]
fn route_requests() {
    const SLIR146: *mut u16 = 0x8700_6924usize as _;
    const SLIR147: *mut u16 = 0x8700_6926usize as _;
    unsafe {
        write_volatile(SLIR146, 0x2d3);
        write_volatile(SLIR147, 0x2d4);
    }
}

#[cfg(feature = "async")]
impl Spi {
    /// Empties both FIFOs (SPFCLR) and clears the receive flags and an
    /// overrun (SPSCLR.SPRFC / OVRFC).
    fn discard_received(&self) {
        const SPSCLR: *mut u16 = (BASE + 0x6a) as _;
        unsafe {
            write_volatile(SPFCLR, 1);
            write_volatile(SPSCLR, 1 << 15 | 1 << 8);
        }
    }

    /// SPCR.SPTIE / SPRIE on or off. On, SPTEF / SPRF are cleared first
    /// (SPSCLR, with the RSPIA disabled): SPTEF, set again once it is
    /// enabled (the transmit FIFO has room), gives the first SPTI0. The
    /// DMAC clears them after each unit. Clearing them after enabling
    /// loses the first SPRF: the receive channel then stops one byte
    /// short.
    fn irq_requests(&self, on: bool) {
        const SPTIE: u32 = 1 << 20;
        const SPRIE: u32 = 1 << 17;
        const SPSCLR: *mut u16 = (BASE + 0x6a) as _;
        const SPRFC: u16 = 1 << 15;
        const SPTEFC: u16 = 1 << 13;
        unsafe {
            let spcr = read_volatile(SPCR);
            if on {
                write_volatile(SPCR, spcr & !SPE);
                write_volatile(SPSCLR, SPRFC | SPTEFC);
                write_volatile(SPCR, spcr | SPTIE | SPRIE | SPE);
            } else {
                write_volatile(SPCR, spcr & !(SPTIE | SPRIE));
            }
        }
    }
}

#[cfg(feature = "async")]
pub use asynch::AsyncSpi;

/// RSPIA0 with two DMA channels, as an embedded-hal-async `SpiBus<u8>`
/// (feature `async`).
///
/// SPTI0 (transmit buffer empty, vector 147) paces the transmit channel,
/// which writes SPDR; SPRI0 (receive buffer full, vector 146) paces the
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
