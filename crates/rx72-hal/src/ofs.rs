//! The option-setting memory (OFSM) that the MCU reads at every reset:
//! OFS0 (the watchdogs' auto-start) and OFS1 (voltage detection 0 and the
//! HOCO at reset), as typed values, and [`option_setting!`](crate::option_setting),
//! which places them, with MDE, at FE7F 5D00h-FE7F 5D0Bh.
//!
//! What this module does not touch, on purpose: MDE stays FFFF FFFFh
//! (little endian, linear bank mode: the code is little endian, and a
//! dual bank mode moves the code flash), and the ID code (OSIS), the flash
//! access window (FAW), the ROM code protection (ROMCODE), SPCC, TMEF and
//! BANKSEL are not written at all: a wrong value there can lock the
//! debugger or the flash out.
//!
//! The values are programmed with the code (the debugger writes the
//! section), and stay until the next program: an application without
//! `option_setting!` leaves what an earlier one wrote. Every bit not set
//! by a method stays 1, its erased value, as the manual asks of the
//! reserved bits.
//!
//! The memory map needs the region: memory.x gets
//!
//! ```text
//! MEMORY { OFSM (r) : ORIGIN = 0xFE7F5D00, LENGTH = 12 }
//! SECTIONS { .rx_option_setting : { KEEP(*(.rx_option_setting)) } > OFSM }
//! ```
//!
//! (test/hw/rx72m-memory.x has it).

use crate::watchdog::{IwdtConfig, WdtConfig};

/// MDE: little endian (MDE = 111b), linear bank mode (BANKMD = 111b), the
/// reserved bits 1: the erased value.
pub const MDE: u32 = 0xffff_ffff;

/// OFS0: the IWDT and the WDT after a reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ofs0(u32);

impl Ofs0 {
    /// Both watchdogs stopped after a reset (register start mode, see
    /// `watchdog`): the erased value.
    pub const DEFAULT: Ofs0 = Ofs0(0xffff_ffff);

    /// The IWDT counting after every reset with `config` (auto-start mode,
    /// IWDTSTRT = 0): IWDTTOPS, IWDTCKS, IWDTRPES, IWDTRPSS and IWDTRSTIRQS
    /// (b3-b12). `Iwdt::start` then reports `Locked`; feed it with an
    /// `Iwdt` from `Iwdt::running`.
    pub const fn iwdt_auto_start(self, config: IwdtConfig) -> Ofs0 {
        let f = (config.timeout as u32) << 2
            | (config.div as u32) << 4
            | (config.window_end as u32) << 8
            | (config.window_start as u32) << 10
            | (config.reset as u32) << 12;
        Ofs0(self.0 & !(1 << 1 | 0x1ffc) | f)
    }

    /// Stops the auto-started IWDT's count in sleep and software standby
    /// modes (IWDTSLCSTP = 1, the erased value); `false` keeps it counting.
    pub const fn iwdt_stop_in_sleep(self, stop: bool) -> Ofs0 {
        Ofs0(if stop { self.0 | 1 << 14 } else { self.0 & !(1 << 14) })
    }

    /// The WDT counting after every reset with `config` (WDTSTRT = 0):
    /// WDTTOPS, WDTCKS, WDTRPES, WDTRPSS and WDTRSTIRQS (b18-b28).
    pub const fn wdt_auto_start(self, config: WdtConfig) -> Ofs0 {
        let f = (config.timeout as u32) << 18
            | (config.div as u32) << 20
            | (config.window_end as u32) << 24
            | (config.window_start as u32) << 26
            | (config.reset as u32) << 28;
        Ofs0(self.0 & !(1 << 17 | 0x1ffc_0000) | f)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }
}

/// The detection level of voltage monitoring 0 (OFS1.VDSEL).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Lvd0Level {
    V2_94 = 0b01,
    V2_87 = 0b10,
    V2_80 = 0b11,
}

/// OFS1: voltage detection 0 and the HOCO after a reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ofs1(u32);

impl Ofs1 {
    /// Voltage monitoring 0 reset off, the HOCO off after a reset: the
    /// erased value.
    pub const DEFAULT: Ofs1 = Ofs1(0xffff_ffff);

    /// Voltage monitoring 0 reset on after every reset (LVDAS = 0), at
    /// `level` (VDSEL): a reset while VCC is below it.
    pub const fn lvd0_reset(self, level: Lvd0Level) -> Ofs1 {
        Ofs1(self.0 & !0b111 | level as u32)
    }

    /// The HOCO oscillating after every reset (HOCOEN = 0).
    pub const fn hoco_at_reset(self) -> Ofs1 {
        Ofs1(self.0 & !(1 << 8))
    }

    pub const fn bits(self) -> u32 {
        self.0
    }
}

/// Places MDE, `ofs0` and `ofs1` in the option-setting memory (section
/// `.rx_option_setting`, which memory.x puts at FE7F 5D00h; see the
/// [module](crate::ofs)). Use it once in the application.
///
/// ```ignore
/// rx72_hal::option_setting!(ofs0: Ofs0::DEFAULT, ofs1: Ofs1::DEFAULT);
/// ```
#[macro_export]
macro_rules! option_setting {
    (ofs0: $ofs0:expr, ofs1: $ofs1:expr $(,)?) => {
        #[unsafe(link_section = ".rx_option_setting")]
        #[unsafe(no_mangle)]
        #[used]
        static RX_OPTION_SETTING: [u32; 3] =
            [$crate::ofs::MDE, $crate::ofs::Ofs0::bits($ofs0), $crate::ofs::Ofs1::bits($ofs1)];
    };
}
