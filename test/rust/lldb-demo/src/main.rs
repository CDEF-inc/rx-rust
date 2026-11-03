//! A program for scripts/check-lldb.sh to debug: calls that are not inlined,
//! with arguments, locals, a struct and a return value, three frames deep
//! below main. Runs on QEMU's RX62N (booted from the reset vector) and on
//! the RX72M.

#![no_std]
#![no_main]
#![allow(non_upper_case_globals)]

use core::hint::black_box;
use core::panic::PanicInfo;
use core::ptr::write_volatile;

#[unsafe(no_mangle)]
static mut result: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;

#[derive(Clone, Copy)]
struct Point {
    x: i32,
    y: i32,
}

#[inline(never)]
fn scale(p: Point, k: i32) -> i32 {
    let sum = p.x * k + p.y * k;
    black_box(sum)
}

#[inline(never)]
fn middle(n: u32) -> u32 {
    let p = Point { x: n as i32, y: -2 };
    let s = scale(p, 3);
    (s as u32).wrapping_add(n)
}

#[inline(never)]
fn outer(a: u32, b: u32) -> u32 {
    let m = middle(a + b);
    black_box(m) * 2
}

#[unsafe(no_mangle)]
fn main() -> ! {
    let r = outer(black_box(4), black_box(1));
    unsafe {
        write_volatile(&raw mut result, r);
        write_volatile(&raw mut finished, 1);
    }
    loop {
        rx_rt::interrupt::wait();
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}
