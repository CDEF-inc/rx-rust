//! rx72-hal's option-setting memory (`hal::ofs`, `option_setting!`) on the
//! AP-RX72M-0A: OFS0 starts the IWDT at the reset (auto-start mode, 16384
//! cycles of IWDTCLK / 256: about 35 s), which the test sees without
//! starting it. `Iwdt::start` is refused (`Locked`), and `Iwdt::running`
//! feeds it. The values in the option-setting memory read back as placed.
//! The next test without its own option setting (hw-board's default)
//! writes the erased values again.
//!
//! Booted from the code flash by scripts/hw-run.sh (scripts/hw-build.sh
//! builds it); the results are left in hw-board's `checks` / `failures` /
//! `first_failure` / `finished`.

#![no_std]
#![no_main]

use core::ptr::read_volatile;

use hw_board::check;
use hw_board::hal::ofs::{MDE, Ofs0, Ofs1};
use hw_board::hal::pac::{self, Peripherals};
use hw_board::hal::watchdog::{IwdtConfig, IwdtDiv, Iwdt, StartError, Timeout};
use rx_rt as _;

const IWDT: IwdtConfig = IwdtConfig::new(IwdtDiv::Div256, Timeout::Cycles16384);
const OFS0: Ofs0 = Ofs0::DEFAULT.iwdt_auto_start(IWDT);

hw_board::hal::option_setting!(ofs0: OFS0, ofs1: Ofs1::DEFAULT);

#[unsafe(no_mangle)]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let rd = |a: usize| unsafe { read_volatile(a as *const u32) };

    // What the option-setting memory holds: MDE erased; OFS0 with
    // IWDTSTRT = 0, TOPS = 11b, CKS = 0101b, no window, reset.
    check(1, rd(0xfe7f_5d00) == MDE);
    check(2, rd(0xfe7f_5d04) == OFS0.bits());
    check(3, OFS0.bits() == 0xffff_fffd & !(0xf << 4) | 0b0101 << 4);
    check(4, rd(0xfe7f_5d08) == Ofs1::DEFAULT.bits());

    // The IWDT counts without a start: down from 3FFFh.
    let mut iwdt = Iwdt::running(unsafe { pac::Iwdt::steal() });
    let a = iwdt.counter();
    let mut spins = 0u32;
    while iwdt.counter() == a && spins < 50_000_000 {
        spins += 1;
    }
    let b = iwdt.counter();
    check(5, a > 0 && a <= 0x3fff && b < a);

    // Its settings are OFS0's: start is refused.
    let other = IwdtConfig::new(IwdtDiv::Div1, Timeout::Cycles1024);
    check(6, matches!(Iwdt::start(p.iwdt, other), Err(StartError::Locked)));

    // A feed starts it over near 3FFFh.
    iwdt.feed();
    let mut spins = 0u32;
    while iwdt.counter() <= b && spins < 50_000_000 {
        spins += 1;
    }
    check(7, iwdt.counter() > b);

    hw_board::finish()
}
