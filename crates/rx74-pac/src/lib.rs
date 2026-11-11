//! Peripheral access crate for the Renesas RX74M and RX74N.
//!
//! The register definitions are not in this repository: build.rs reads
//! RX74M.sfrx or RX74N.sfrx (feature `rx74m` or `rx74n`, exactly one), the
//! register description that Renesas e2 studio installs for its debugger,
//! turns it into an SVD description and generates the code with svd2rust
//! (`--target none`) into `OUT_DIR`. See sfrx-pac for where the file is
//! looked for (or set `RX74M_SFRX` / `RX74N_SFRX`). The files come with the
//! RX FSP packs (docs/NOTES-hw.md).
//!
//! The two have the same registers at the same addresses, except that the
//! RX74N has no EtherCAT slave controller (ESC), no DSMIF and the few bits
//! for them (docs/NOTES-hal.md lists them).
//!
//! The registers are at their secure addresses, as a reset leaves them; the
//! `_NS` aliases are left out.
//!
//! The sfrx file has every register as read-write and no reset values, so
//! all registers are read-write here and `reset()` / `write()` start from
//! 0. It has no interrupt vectors either; use rx-rt's `interrupt!` with the
//! vector numbers from the hardware manual.

#![no_std]
#![allow(non_camel_case_types, non_snake_case, clippy::all)]
#![allow(rustdoc::all)]

include!(concat!(env!("OUT_DIR"), "/pac.rs"));
