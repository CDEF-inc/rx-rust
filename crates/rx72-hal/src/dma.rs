//! The DMA controller (DMAC): eight channels in normal transfer mode, from
//! memory to memory (started by software) or paced by an interrupt
//! request (a peripheral's "data empty" or "data full", a timer's compare
//! match).
//!
//! - [`Dma::new`] takes the DMAC out of module stop and starts it
//!   (DMAST.DMST); it returns the channels [`Channel<0>`](Channel) to
//!   `Channel<7>`.
//! - [`Channel::copy`] copies a slice to another and waits (bounded) for
//!   the end; with the feature `async`, [`Channel::copy_async`] waits for
//!   the transfer end interrupt instead.
//! - [`Channel::start`] (unsafe: raw addresses) starts any [`Transfer`],
//!   for example from a buffer to a peripheral's data register, one unit
//!   per request of an [`Interrupt`](crate::icu::Interrupt).
//!   [`Channel::is_done`], [`Channel::remaining`] and [`Channel::stop`]
//!   follow it; with `async`, [`Channel::done`] waits for it.
//!
//! A transfer of up to 65535 units of 8, 16 or 32 bits; each address is
//! fixed, incremented or decremented by the unit after each one.
//!
//! **Interrupt-paced transfers:** DMRSRn names the source by its vector
//! number. While the channel is enabled (DMCNT.DTE) the source's requests
//! go to the DMAC, not the CPU: enable the source (IER) so that the
//! request reaches the DMAC at all (its priority does not matter). Once
//! the count is done DTE clears, and the source's next requests go to the
//! CPU again: stop the source, disable it, or give it a handler.
//!
//! **Transfer end interrupts (feature `async`):** DMAC0I-DMAC3I (vectors
//! 120-123) for channels 0-3, and DMAC74I (vector 124, which DMIST tells
//! apart) for channels 4-7. [`enable_interrupts`] sets their priority and
//! enables them; register the handlers with
//! [`bind_dma_interrupts!`](crate::bind_dma_interrupts).

use core::marker::PhantomData;
use core::ptr::{read_volatile, write_volatile};

use crate::icu::Interrupt;
use crate::mstp::{self, Mstpcr};
use crate::pac;
use crate::timeout::{self, Timeout};

/// DMAC0's registers; channel n is at + 40h × n.
const DMAC0: usize = 0x0008_2000;
/// The common registers: DMAST, DMIST.
const DMAST: *mut u8 = 0x0008_2200 as _;
const DMIST: *const u8 = 0x0008_2204 as _;
/// DMRSRn (activation source, the vector number) = ICU + 400h + 4 × n.
const DMRSR0: usize = 0x0008_7400;
/// The DMAC / DTC module stop bit (MSTPCRA.MSTPA28).
const MSTP: (Mstpcr, u32) = (Mstpcr::A, 28);
/// The transfer end interrupts: DMAC0I-DMAC3I, then DMAC74I.
const VECTOR0: u8 = 120;

// Register offsets within a channel.
const DMSAR: usize = 0x00;
const DMDAR: usize = 0x04;
const DMCRA: usize = 0x08;
const DMTMD: usize = 0x10;
const DMINT: usize = 0x13;
const DMAMD: usize = 0x14;
const DMCNT: usize = 0x1c;
const DMREQ: usize = 0x1d;
const DMSTS: usize = 0x1e;

const DMINT_DTIE: u8 = 1 << 4;
const DMREQ_SWREQ: u8 = 1 << 0;
/// SWREQ stays set after a transfer: one request for the whole count.
const DMREQ_CLRS: u8 = 1 << 4;
const DMSTS_DTIF: u8 = 1 << 4;
const DMSTS_ACT: u8 = 1 << 7;

/// The unit of a transfer (DMTMD.SZ).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    U8 = 0,
    U16 = 1,
    U32 = 2,
}

/// What happens to an address after each unit (DMAMD.SM / DM).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Fixed = 0,
    Increment = 2,
    Decrement = 3,
}

/// What starts the units (DMTMD.DCTG).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// DMREQ.SWREQ: the whole count at once.
    Software,
    /// One unit per request of the source (DMRSRn = its vector).
    Interrupt(Interrupt),
}

/// A transfer for [`Channel::start`]: `count` units of `size` from `src`
/// to `dst`.
#[derive(Clone, Copy, Debug)]
pub struct Transfer {
    pub src: usize,
    pub src_step: Step,
    pub dst: usize,
    pub dst_step: Step,
    pub size: Size,
    /// 1 to 65535.
    pub count: u16,
    pub trigger: Trigger,
}

/// Why a transfer did not run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The slices differ in length, or are empty or longer than 65535.
    Length,
    /// The transfer did not end within the bound.
    Timeout,
}

impl From<Timeout> for Error {
    fn from(_: Timeout) -> Error {
        Error::Timeout
    }
}

/// A unit a slice copy moves: `u8`, `u16` or `u32`.
pub trait Word: Copy + sealed::Sealed {
    const SIZE: Size;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for u8 {}
    impl Sealed for u16 {}
    impl Sealed for u32 {}
}

impl Word for u8 {
    const SIZE: Size = Size::U8;
}
impl Word for u16 {
    const SIZE: Size = Size::U16;
}
impl Word for u32 {
    const SIZE: Size = Size::U32;
}

/// The eight channels.
pub struct Dma {
    pub ch0: Channel<0>,
    pub ch1: Channel<1>,
    pub ch2: Channel<2>,
    pub ch3: Channel<3>,
    pub ch4: Channel<4>,
    pub ch5: Channel<5>,
    pub ch6: Channel<6>,
    pub ch7: Channel<7>,
}

impl Dma {
    /// Takes the DMAC out of module stop and starts it (DMAST.DMST = 1).
    /// The channels' own registers are written by each transfer.
    pub fn new(_dmac: pac::Dmac) -> Dma {
        mstp::start(MSTP.0, MSTP.1);
        unsafe { write_volatile(DMAST, 1) };
        Dma {
            ch0: Channel(PhantomData),
            ch1: Channel(PhantomData),
            ch2: Channel(PhantomData),
            ch3: Channel(PhantomData),
            ch4: Channel(PhantomData),
            ch5: Channel(PhantomData),
            ch6: Channel(PhantomData),
            ch7: Channel(PhantomData),
        }
    }
}

/// DMA channel `N` (0-7).
pub struct Channel<const N: u8>(PhantomData<*mut ()>);

unsafe impl<const N: u8> Send for Channel<N> {}

fn reg8(n: u8, off: usize) -> *mut u8 {
    (DMAC0 + 0x40 * n as usize + off) as _
}

fn reg16(n: u8, off: usize) -> *mut u16 {
    (DMAC0 + 0x40 * n as usize + off) as _
}

fn reg32(n: u8, off: usize) -> *mut u32 {
    (DMAC0 + 0x40 * n as usize + off) as _
}

fn dmrsr(n: u8) -> *mut u8 {
    (DMRSR0 + 4 * n as usize) as _
}

/// The transfer end interrupt of channel `n`.
fn end_interrupt(n: u8) -> Interrupt {
    let v = VECTOR0 + if n < 4 { n } else { 4 };
    unsafe { Interrupt::new(v, v) }
}

impl<const N: u8> Channel<N> {
    /// Starts `t`. A transfer still running on this channel is stopped
    /// first.
    ///
    /// # Safety
    ///
    /// `t.src` / `t.dst` and the units after them (by the steps) must be
    /// readable / writable for the whole transfer: the DMAC accesses them
    /// until the count is done or [`Channel::stop`].
    pub unsafe fn start(&mut self, t: &Transfer) {
        self.stop();
        let n = N;
        let (dctg, source) = match t.trigger {
            Trigger::Software => (0u16, 0u8),
            Trigger::Interrupt(i) => (1, i.vector()),
        };
        unsafe {
            write_volatile(reg8(n, DMSTS), 0);
            write_volatile(dmrsr(n), source);
            write_volatile(reg32(n, DMSAR), t.src as u32);
            write_volatile(reg32(n, DMDAR), t.dst as u32);
            write_volatile(reg32(n, DMCRA), t.count as u32);
            // Normal transfer mode (MD = 00b), no repeat area (DTS = 10b).
            write_volatile(reg16(n, DMTMD), 0b10 << 12 | (t.size as u16) << 8 | dctg);
            write_volatile(
                reg16(n, DMAMD),
                (t.src_step as u16) << 14 | (t.dst_step as u16) << 6,
            );
            write_volatile(reg8(n, DMINT), 0);
            write_volatile(reg8(n, DMCNT), 1);
            if dctg == 0 {
                write_volatile(reg8(n, DMREQ), DMREQ_CLRS | DMREQ_SWREQ);
            }
        }
    }

    /// Whether the last transfer has ended (DMSTS.DTIF): its count is done.
    pub fn is_done(&self) -> bool {
        unsafe { read_volatile(reg8(N, DMSTS)) & DMSTS_DTIF != 0 }
    }

    /// The units not transferred yet (DMCRA).
    pub fn remaining(&self) -> u16 {
        unsafe { read_volatile(reg32(N, DMCRA)) as u16 }
    }

    /// Disables the channel (DMCNT.DTE = 0) and waits for a unit in
    /// progress to end (DMSTS.ACT); a software request is withdrawn.
    pub fn stop(&mut self) {
        unsafe {
            write_volatile(reg8(N, DMCNT), 0);
            write_volatile(reg8(N, DMREQ), 0);
            let _ = timeout::wait(100_000, || read_volatile(reg8(N, DMSTS)) & DMSTS_ACT == 0);
            write_volatile(reg8(N, DMINT), 0);
        }
    }

    /// Copies `src` to `dst` (the same length, 1 to 65535 units) and waits
    /// for the end. The CPU waits too, so this only helps where the copy
    /// leaves the bus to other masters; see [`Channel::copy_async`].
    pub fn copy<T: Word>(&mut self, src: &[T], dst: &mut [T]) -> Result<(), Error> {
        let t = slice_transfer(src, dst)?;
        unsafe { self.start(&t) };
        let r = timeout::wait(10_000_000, || self.is_done());
        self.stop();
        Ok(r?)
    }
}

fn slice_transfer<T: Word>(src: &[T], dst: &mut [T]) -> Result<Transfer, Error> {
    if src.len() != dst.len() || src.is_empty() || src.len() > 0xffff {
        return Err(Error::Length);
    }
    Ok(Transfer {
        src: src.as_ptr() as usize,
        src_step: Step::Increment,
        dst: dst.as_mut_ptr() as usize,
        dst_step: Step::Increment,
        size: T::SIZE,
        count: src.len() as u16,
        trigger: Trigger::Software,
    })
}

#[cfg(feature = "async")]
pub use asynch::*;

#[cfg(feature = "async")]
mod asynch {
    use core::cell::RefCell;
    use core::future::poll_fn;
    use core::ptr::{read_volatile, write_volatile};
    use core::task::{Poll, Waker};

    use critical_section::Mutex;

    use super::{
        Channel, DMINT, DMINT_DTIE, DMIST, Error, Word, end_interrupt, reg8, slice_transfer,
    };
    use crate::icu::Priority;

    static WAKERS: Mutex<RefCell<[Option<Waker>; 8]>> =
        Mutex::new(RefCell::new([const { None }; 8]));

    /// Sets the priority of the transfer end interrupts (vectors 120-124)
    /// and enables them. Each channel enables its own (DMINT.DTIE) while a
    /// future waits.
    pub fn enable_interrupts(priority: Priority) {
        for n in 0..5 {
            let i = end_interrupt(n);
            i.clear_pending();
            i.set_priority(priority);
            i.enable();
        }
    }

    fn on_channel(n: u8) {
        // DTIE off, so that the request does not come again; DTIF stays
        // for the future to see.
        unsafe { write_volatile(reg8(n, DMINT), 0) };
        critical_section::with(|cs| {
            if let Some(w) = WAKERS.borrow_ref_mut(cs)[n as usize].take() {
                w.wake();
            }
        });
    }

    /// The handler of DMAC0I (vector 120).
    pub extern "C" fn on_dmac0() {
        on_channel(0);
    }

    /// The handler of DMAC1I (vector 121).
    pub extern "C" fn on_dmac1() {
        on_channel(1);
    }

    /// The handler of DMAC2I (vector 122).
    pub extern "C" fn on_dmac2() {
        on_channel(2);
    }

    /// The handler of DMAC3I (vector 123).
    pub extern "C" fn on_dmac3() {
        on_channel(3);
    }

    /// The handler of DMAC74I (vector 124): DMIST says which of channels
    /// 4-7 requested it.
    pub extern "C" fn on_dmac74() {
        let ist = unsafe { read_volatile(DMIST) };
        for n in 4..8 {
            if ist & 1 << n != 0 {
                on_channel(n);
            }
        }
    }

    /// Stops the channel when a future is dropped before the end.
    struct StopOnDrop<'a, const N: u8>(&'a mut Channel<N>);

    impl<const N: u8> Drop for StopOnDrop<'_, N> {
        fn drop(&mut self) {
            self.0.stop();
        }
    }

    impl<const N: u8> Channel<N> {
        /// Waits for the transfer started by [`Channel::start`] to end,
        /// with the transfer end interrupt
        /// ([`enable_interrupts`], [`bind_dma_interrupts!`](crate::bind_dma_interrupts)).
        pub async fn done(&mut self) {
            poll_fn(|cx| {
                if self.is_done() {
                    return Poll::Ready(());
                }
                critical_section::with(|cs| {
                    WAKERS.borrow_ref_mut(cs)[N as usize] = Some(cx.waker().clone());
                });
                unsafe { write_volatile(reg8(N, DMINT), DMINT_DTIE) };
                // It may have ended before DTIE was set.
                if self.is_done() {
                    unsafe { write_volatile(reg8(N, DMINT), 0) };
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await
        }

        /// Copies `src` to `dst` (the same length, 1 to 65535 units) and
        /// waits for the transfer end interrupt. Dropping the future stops
        /// the transfer.
        pub async fn copy_async<T: Word>(&mut self, src: &[T], dst: &mut [T]) -> Result<(), Error> {
            let t = slice_transfer(src, dst)?;
            unsafe { self.start(&t) };
            let guard = StopOnDrop(self);
            guard.0.done().await;
            drop(guard);
            Ok(())
        }
    }
}

/// Registers the transfer end handlers (rx-rt's `interrupt!`): vectors
/// 120-123 for channels 0-3, 124 for channels 4-7. Use it once, with the
/// feature `async`.
#[cfg(feature = "async")]
#[macro_export]
macro_rules! bind_dma_interrupts {
    () => {
        rx_rt::interrupt!(120, $crate::dma::on_dmac0);
        rx_rt::interrupt!(121, $crate::dma::on_dmac1);
        rx_rt::interrupt!(122, $crate::dma::on_dmac2);
        rx_rt::interrupt!(123, $crate::dma::on_dmac3);
        rx_rt::interrupt!(124, $crate::dma::on_dmac74);
    };
}
