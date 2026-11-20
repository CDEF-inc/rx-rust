//! The HAL's DMA controller driver (`hal::dma`) on the board, at the
//! boards' PLL clocks:
//!
//! - slice copies of 8, 16 and 32 bits on several channels, and the length
//!   checks;
//! - a fill (fixed source) and a reversed copy (decrementing source);
//! - a transfer paced by CMT0's compare match (CMI0, vector 28) at 1 kHz:
//!   one unit per request, none of the requests reaching the CPU while the
//!   channel is enabled;
//! - with the feature `async`: `copy_async` on channel 3 (DMAC3I, vector
//!   123) and channel 5 (DMAC74I, vector 124), and `done` on CMI0-paced
//!   transfers on channels 3 and 4 (which end in the interrupt), with
//!   rx-rt's executor asleep meanwhile.
//!
//! Booted from the code flash / MRAM by scripts/hw-run.sh (scripts/hw-build.sh
//! builds it); the results are left in hw-board's `checks` / `failures` /
//! `first_failure` / `finished`.

#![no_std]
#![no_main]

use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use embedded_hal::delay::DelayNs;
use hw_board::check;
use hw_board::hal::{
    self,
    delay::Delay,
    dma::{self, Dma, Error, Size, Step, Transfer, Trigger},
    icu::{Interrupt, Priority},
    pac::{self, Peripherals},
};
use rx_rt::executor::{Executor, Spawner};

hal::bind_dma_interrupts!();
rx_rt::interrupt!(28, on_cmi0);

/// CMI0s that reached the CPU.
static CPU_CMI0: AtomicU32 = AtomicU32::new(0);

extern "C" fn on_cmi0() {
    CPU_CMI0.fetch_add(1, Relaxed);
}

struct Board {
    dma: Dma,
    delay: Delay,
    cmt: pac::Cmt,
    cmt0: pac::Cmt0,
}

/// CMT0's compare match value for 1 kHz at PCLK / 512.
#[cfg(feature = "ap-rx72m")]
const CMCOR_1KHZ: u16 = (60_000_000 / 512 / 1000 - 1) as u16;
#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
const CMCOR_1KHZ: u16 = (75_000_000 / 512 / 1000 - 1) as u16;

#[cfg(feature = "ap-rx72m")]
fn setup(p: Peripherals) -> Board {
    let clocks = hal::clock::Config::pll_240mhz_from_24mhz().configure(&p.system);
    // CMT unit 0 out of module stop. MSTPCRA is protected by PRCR.PRC1,
    // which the clock setup leaves 0.
    p.system.prcr().write(|w| unsafe { w.bits(0xa502) });
    p.system.mstpcra().modify(|_, w| w.mstpa15().clear_bit());
    p.system.prcr().write(|w| unsafe { w.bits(0xa500) });
    Board { dma: Dma::new(p.dmac), delay: Delay::new(p.cmt2, &clocks), cmt: p.cmt, cmt0: p.cmt0 }
}
#[cfg(any(feature = "ek-rx74n", feature = "ek-rx74m"))]
fn setup(p: Peripherals) -> Board {
    let clocks = hal::clock::Config::pll_600mhz_from_24mhz().configure(&p.sysc, &p.sram, &p.mram);
    p.mstpreg.mstpcrf().modify(|_, w| w.mstpf15().clear_bit());
    Board { dma: Dma::new(p.dmac), delay: Delay::new(p.cmt2, &clocks), cmt: p.cmt, cmt0: p.cmt0 }
}

/// CMT0 at 1 kHz with its compare match interrupt request (CMIE), and CMI0
/// enabled in the ICU, so that its requests reach the DMAC.
fn cmt0_start(cmt: &pac::Cmt, cmt0: &pac::Cmt0) {
    cmt.cmstr0().modify(|_, w| w.str0().clear_bit());
    cmt0.cmcnt().write(|w| unsafe { w.bits(0) });
    cmt0.cmcor().write(|w| unsafe { w.bits(CMCOR_1KHZ) });
    cmt0.cmcr().write(|w| unsafe { w.cks().bits(3) }.cmie().set_bit());
    Interrupt::CMI0.clear_pending();
    Interrupt::CMI0.set_priority(Priority::P1);
    Interrupt::CMI0.enable();
    cmt.cmstr0().modify(|_, w| w.str0().set_bit());
}

fn cmt0_stop(cmt: &pac::Cmt) {
    cmt.cmstr0().modify(|_, w| w.str0().clear_bit());
    Interrupt::CMI0.disable();
    Interrupt::CMI0.clear_pending();
}

/// Waits (bounded) for `f`, in steps of 100 µs.
fn wait_for(delay: &mut Delay, ms: u32, mut f: impl FnMut() -> bool) -> bool {
    for _ in 0..ms * 10 {
        if f() {
            return true;
        }
        delay.delay_us(100);
    }
    f()
}

static mut SRC8: [u8; 256] = [0; 256];
static mut DST8: [u8; 256] = [0; 256];
static mut SRC16: [u16; 64] = [0; 64];
static mut DST16: [u16; 64] = [0; 64];
static mut SRC32: [u32; 32] = [0; 32];
static mut DST32: [u32; 32] = [0; 32];

fn blocking(b: &mut Board) {
    let (s8, d8, s16, d16, s32, d32) = unsafe {
        (
            &mut *(&raw mut SRC8),
            &mut *(&raw mut DST8),
            &mut *(&raw mut SRC16),
            &mut *(&raw mut DST16),
            &mut *(&raw mut SRC32),
            &mut *(&raw mut DST32),
        )
    };
    for (i, x) in s8.iter_mut().enumerate() {
        *x = (i as u8).wrapping_mul(7).wrapping_add(3);
    }
    for (i, x) in s16.iter_mut().enumerate() {
        *x = 0xa500 | i as u16;
    }
    for (i, x) in s32.iter_mut().enumerate() {
        *x = 0x1234_0000 + (i as u32) * 0x0101;
    }

    // Copies of each size, on channels 0, 1 and 7.
    check(1, b.dma.ch0.copy(&s8[..100], &mut d8[..100]) == Ok(()));
    check(2, d8[..100] == s8[..100] && d8[100] == 0);
    check(3, b.dma.ch1.copy(&s16[..], &mut d16[..]) == Ok(()) && d16 == s16);
    check(4, b.dma.ch7.copy(&s32[..], &mut d32[..]) == Ok(()) && d32 == s32);

    // Lengths: different, empty.
    check(5, b.dma.ch0.copy(&s8[..3], &mut d8[..4]) == Err(Error::Length));
    check(6, b.dma.ch0.copy(&s8[..0], &mut d8[..0]) == Err(Error::Length));

    // A fill: a fixed source.
    let pattern: u32 = 0xdead_beef;
    let t = Transfer {
        src: &pattern as *const u32 as usize,
        src_step: Step::Fixed,
        dst: d32.as_mut_ptr() as usize,
        dst_step: Step::Increment,
        size: Size::U32,
        count: 16,
        trigger: Trigger::Software,
    };
    unsafe { b.dma.ch2.start(&t) };
    let done = wait_for(&mut b.delay, 10, || b.dma.ch2.is_done());
    check(7, done && b.dma.ch2.remaining() == 0);
    check(8, d32[..16].iter().all(|&x| x == pattern) && d32[16] == s32[16]);

    // Reversed: the source decrements from its last byte.
    let t = Transfer {
        src: unsafe { s8.as_ptr().add(9) } as usize,
        src_step: Step::Decrement,
        dst: d8.as_mut_ptr() as usize,
        dst_step: Step::Increment,
        size: Size::U8,
        count: 10,
        trigger: Trigger::Software,
    };
    unsafe { b.dma.ch2.start(&t) };
    let done = wait_for(&mut b.delay, 10, || b.dma.ch2.is_done());
    check(9, done && (0..10).all(|i| d8[i] == s8[9 - i]));

    // Paced by CMI0 at 1 kHz: 10 units take about 10 ms, and none of the
    // requests reaches the CPU meanwhile.
    d32.fill(0);
    let t = Transfer {
        src: s32.as_ptr() as usize,
        src_step: Step::Increment,
        dst: d32.as_mut_ptr() as usize,
        dst_step: Step::Increment,
        size: Size::U32,
        count: 10,
        trigger: Trigger::Interrupt(Interrupt::CMI0),
    };
    CPU_CMI0.store(0, Relaxed);
    unsafe { b.dma.ch2.start(&t) };
    cmt0_start(&b.cmt, &b.cmt0);
    b.delay.delay_ms(4);
    let mid = b.dma.ch2.remaining();
    check(10, (4..=8).contains(&mid) && !b.dma.ch2.is_done());
    let done = wait_for(&mut b.delay, 20, || b.dma.ch2.is_done());
    let cpu = CPU_CMI0.load(Relaxed);
    cmt0_stop(&b.cmt);
    check(11, done && d32[..10] == s32[..10] && d32[10] == 0);
    check(12, cpu == 0);
}

/// A CMI0-paced transfer of 20 units on `ch`, awaited: whether it moved
/// them all and none of CMT0's requests reached the CPU meanwhile.
async fn paced<const N: u8>(
    ch: &mut dma::Channel<N>,
    cmt: &pac::Cmt,
    cmt0: &pac::Cmt0,
    s32: &[u32; 32],
    d32: &mut [u32; 32],
) -> bool {
    d32.fill(0);
    let t = Transfer {
        src: s32.as_ptr() as usize,
        src_step: Step::Increment,
        dst: d32.as_mut_ptr() as usize,
        dst_step: Step::Increment,
        size: Size::U32,
        count: 20,
        trigger: Trigger::Interrupt(Interrupt::CMI0),
    };
    CPU_CMI0.store(0, Relaxed);
    unsafe { ch.start(&t) };
    cmt0_start(cmt, cmt0);
    ch.done().await;
    let cpu = CPU_CMI0.load(Relaxed);
    cmt0_stop(cmt);
    ch.is_done() && d32[..20] == s32[..20] && d32[20] == 0 && cpu == 0
}

#[embassy_executor::task]
async fn run(_spawner: Spawner, mut b: Board) {
    blocking(&mut b);

    let (s8, d8, s32, d32) = unsafe {
        (&mut *(&raw mut SRC8), &mut *(&raw mut DST8), &mut *(&raw mut SRC32), &mut *(&raw mut DST32))
    };
    dma::enable_interrupts(Priority::new(3).unwrap());

    // copy_async on channel 3 (DMAC3I) and 5 (DMAC74I).
    d8.fill(0);
    check(13, b.dma.ch3.copy_async(&s8[..], &mut d8[..]).await == Ok(()));
    check(14, d8 == s8);
    d8.fill(0);
    check(15, b.dma.ch5.copy_async(&s8[..200], &mut d8[..200]).await == Ok(()));
    check(16, d8[..200] == s8[..200] && d8[200] == 0);

    // done on CMI0-paced transfers (20 units, about 20 ms, the executor
    // asleep meanwhile) on channel 3 (DMAC3I) and 4 (DMAC74I): the copies
    // above may end before DTIE is set, these end in the interrupt.
    check(17, paced(&mut b.dma.ch3, &b.cmt, &b.cmt0, s32, d32).await);
    check(18, paced(&mut b.dma.ch4, &b.cmt, &b.cmt0, s32, d32).await);

    hw_board::finish()
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let b = setup(Peripherals::take().unwrap());
    static mut EXECUTOR: MaybeUninit<Executor> = MaybeUninit::uninit();
    let executor = unsafe { (&raw mut EXECUTOR).as_mut().unwrap().write(Executor::new()) };
    executor.run(|spawner| spawner.spawn(run(spawner, b).unwrap()))
}
