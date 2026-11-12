// after_stack.ll in Rust, without core (no RX std needed): the int follows a
// structure that is always on the stack (scripts/check-abi-note.sh).
#![feature(no_core, lang_items)]
#![no_core]
#[lang = "pointee_sized"]
pub trait PointeeSized {}
#[lang = "meta_sized"]
pub trait MetaSized: PointeeSized {}
#[lang = "sized"]
pub trait Sized: MetaSized {}
#[lang = "copy"]
pub trait Copy {}

#[repr(C)]
pub struct S6 {
    pub c: [u16; 3],
}

#[no_mangle]
pub extern "C" fn after_stack(_s: S6, x: i32) -> i32 {
    x
}
