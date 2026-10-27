//! Peripheral access crate for the Renesas RX72M.
//!
//! The register definitions are not in this repository: build.rs reads
//! RX72M.sfrx, the register description that Renesas e2 studio installs for
//! its debugger, turns it into an SVD description and generates the code
//! with svd2rust (`--target none`) into `OUT_DIR`. See build.rs for where
//! the file is looked for (or set `RX72M_SFRX`).
//!
//! The sfrx file has every register as read-write and no reset values, so
//! all registers are read-write here and `reset()` / `write()` start from
//! 0. It has no interrupt vectors either; use rx-rt's `interrupt!` with the
//! vector numbers from the hardware manual.

#![no_std]
#![allow(non_camel_case_types, non_snake_case, clippy::all)]
#![allow(rustdoc::all)]

include!(concat!(env!("OUT_DIR"), "/pac.rs"));
