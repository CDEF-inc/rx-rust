//! rx74-hal on the EK-RX74N board: GPIO (the user LEDs LED1 / LED2 / LED3 on
//! PF05 / P807 / P604, the pull-up on SW1's P414) and `DelayNs` against CMT0.
//! The LEDs blink in turn for about two seconds. Booted from the code MRAM by
//! scripts/hw-run.sh (built by scripts/hw-build.sh); the results are left in
//! `checks` / `failures` / `first_failure` / `finished`.

use core::ptr::write_volatile;

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{InputPin, OutputPin, StatefulOutputPin};
use hw_board::hal::clock::Clocks;
use hw_board::hal::delay::Delay;
use hw_board::hal::gpio::Parts;
use hw_board::hal::pac::Peripherals;
use hw_board::check;

// CMT0 counts during a 100 ms delay, for the delay check.
#[unsafe(no_mangle)]
static mut counted: u32 = 0;

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = Clocks::after_reset();
    let pins = Parts::take().unwrap();
    check(1, Parts::take().is_none());

    // --- GPIO
    let mut led1 = pins.pf05.into_output(true);
    let mut led2 = pins.p807.into_push_pull_output();
    let mut led3 = pins.p604.into_push_pull_output();
    check(2, led1.is_set_high().unwrap());
    check(3, led1.is_high().unwrap());
    check(4, led2.is_set_low().unwrap());
    check(5, led2.is_low().unwrap());
    led2.set_high().unwrap();
    led1.set_low().unwrap();
    check(6, led2.is_high().unwrap() && led1.is_low().unwrap());
    led2.toggle().unwrap();
    check(7, led2.is_low().unwrap());
    // SW1 is not pressed, and the pull-up holds its pin high.
    let mut sw1 = pins.p414.into_pull_up_input();
    check(8, sw1.is_high().unwrap());

    // --- the delay against CMT0 at PCLKB / 512 (15625 Hz), on CMT unit 0
    let prcr = &p.sysc;
    prcr.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) }.prc1().set_bit());
    p.mstpreg.mstpcrf().modify(|_, w| w.mstpf15().clear_bit());
    prcr.prcr_s().write(|w| unsafe { w.prkey().bits(0xa5) });
    p.cmt0.cmcor().write(|w| unsafe { w.bits(0xffff) });
    p.cmt0.cmcr().write(|w| unsafe { w.cks().bits(3) });
    p.cmt.cmstr0().write(|w| w.str0().set_bit());
    let mut delay = Delay::new(p.cmt2, &clocks);
    let a = p.cmt0.cmcnt().read().bits();
    delay.delay_ms(100);
    let n = p.cmt0.cmcnt().read().bits().wrapping_sub(a) as u32;
    unsafe { write_volatile(&raw mut counted, n) };
    check(9, (1500..1650).contains(&n));

    // --- the LEDs in turn
    for i in 0..30 {
        led1.set_state((i % 3 == 0).into()).unwrap();
        led2.set_state((i % 3 == 1).into()).unwrap();
        led3.set_state((i % 3 == 2).into()).unwrap();
        delay.delay_ms(66);
    }
    led1.set_low().unwrap();
    led2.set_low().unwrap();
    led3.set_low().unwrap();
    check(10, sw1.is_high().unwrap());

    hw_board::finish()
}
