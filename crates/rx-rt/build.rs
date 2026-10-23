//! Generates the relocatable vector table and `link.x`, and puts `link.x` on
//! the linker search path. The application supplies `memory.x`.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

/// Exceptions in the fixed vector table (offsets from 0xFFFFFF80). Every
/// other word is left at 0xFFFFFFFF: on most parts 0xFFFFFF80-0xFFFFFFCF
/// holds option-setting memory and ID codes, where all ones is the
/// "disabled / little endian" default.
const EXCEPTIONS: &[(u32, &str)] = &[
    (0x50, "privileged"),
    (0x54, "access"),
    (0x5c, "undefined"),
    (0x64, "fpu"),
    (0x78, "nmi"),
];

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    let mut s = String::new();
    s.push_str("\t.pushsection .rx_vectors,\"a\"\n\t.balign 4\n\t.global _rx_vectors\n_rx_vectors:\n");
    for n in 0..256 {
        writeln!(s, "\t.long\t_rx_vector_{n}").unwrap();
    }
    s.push_str("\t.popsection\n");
    s.push_str("\t.pushsection .rx_fixed_vectors,\"a\"\n\t.global _rx_fixed_vectors\n_rx_fixed_vectors:\n");
    for off in (0..0x80).step_by(4) {
        match EXCEPTIONS.iter().find(|e| e.0 == off) {
            Some((_, name)) => writeln!(s, "\t.long\t_rx_exception_{name}").unwrap(),
            None if off == 0x7c => s.push_str("\t.long\t_rx_reset\n"),
            None => s.push_str("\t.long\t0xffffffff\n"),
        }
    }
    s.push_str("\t.popsection\n");
    fs::write(out.join("vectors.s"), s).unwrap();

    let mut provide = String::new();
    for n in 0..256 {
        writeln!(provide, "PROVIDE(_rx_vector_{n} = _rx_default_handler);").unwrap();
    }
    for (_, name) in EXCEPTIONS {
        writeln!(provide, "PROVIDE(_rx_exception_{name} = _rx_default_exception);").unwrap();
    }
    let link = fs::read_to_string("link.x.in").unwrap().replace("@PROVIDE@", &provide);
    fs::write(out.join("link.x"), link).unwrap();

    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=link.x.in");
}
