//! `alloc` on rx-none-elf: a global allocator (embedded-alloc's linked-list
//! heap on a static array, which takes rx-rt's critical-section
//! implementation) and Box, Vec, String / format!, BTreeMap, VecDeque,
//! BinaryHeap, Rc and Arc. Built for QEMU's rx62n (RXv1) by default;
//! scripts/run-rust-rom.sh test/rust/alloc-smoke, with RX_CPU for the other
//! cores. The results are left in `checks` / `failures` / `finished` for
//! the QEMU monitor; scripts/hw-build.sh also builds it for the RX72M board
//! (rxv3), where scripts/hw-run.sh stops it at `done`.

#![no_std]
#![no_main]
// The result variables keep the names the QEMU scripts read.
#![allow(non_upper_case_globals)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BinaryHeap, VecDeque};
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::panic::PanicInfo;
use core::ptr::write_volatile;

use embedded_alloc::LlffHeap as Heap;

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut first_failure: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;

fn check(id: u32, ok: bool) {
    unsafe {
        checks += 1;
        if !ok {
            if failures == 0 {
                first_failure = id;
            }
            failures += 1;
        }
    }
}

#[global_allocator]
static HEAP: Heap = Heap::empty();

const HEAP_SIZE: usize = 16 * 1024;
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

#[repr(align(16))]
struct Aligned16([u32; 4]);

// Keeps the optimizer from computing the results at compile time.
fn opaque<T>(x: T) -> T {
    core::hint::black_box(x)
}

fn boxes() {
    let b = Box::new(opaque(0x1234_5678u32));
    check(1, *b == 0x1234_5678);
    // u64 is 4-byte aligned on RX (as in GCC); an over-aligned type gets
    // its alignment from the allocator.
    let w = Box::new(opaque(0x0123_4567_89ab_cdefu64));
    check(2, *w == 0x0123_4567_89ab_cdef && (&*w as *const u64 as usize) % 4 == 0);
    for i in 0..8u32 {
        let a = Box::new(Aligned16([i, i + 1, i + 2, i + 3]));
        let p = &*a as *const Aligned16 as usize;
        check(3, p % 16 == 0 && a.0[3] == i + 3);
    }
    let s: Box<[u16]> = (0..100u16).collect();
    check(4, s.len() == 100 && s.iter().map(|&x| x as u32).sum::<u32>() == 4950);
    let d: Box<dyn Fn(u32) -> u32> = Box::new(move |x| x * opaque(3));
    check(5, d(14) == 42);
}

fn vectors() {
    let mut v = Vec::new();
    for i in 0..1000u32 {
        v.push(opaque(i * 7 % 1000));
    }
    check(10, v.len() == 1000 && v.iter().sum::<u32>() == 499_500);
    v.sort_unstable();
    check(11, v.windows(2).all(|w| w[0] <= w[1]) && v[0] == 0 && v[999] == 999);
    v.retain(|&x| x % 3 == 0);
    check(12, v.len() == 334);
    v.truncate(10);
    v.shrink_to_fit();
    check(13, v.capacity() >= 10 && v == [0, 3, 6, 9, 12, 15, 18, 21, 24, 27]);
    let mut bytes: Vec<u8> = Vec::with_capacity(3);
    bytes.extend_from_slice(b"rx-none-elf");
    bytes.insert(0, b'<');
    bytes.push(b'>');
    check(14, &bytes[..] == b"<rx-none-elf>");
    let nested: Vec<Vec<u64>> = (0..20u64).map(|i| (0..i).collect()).collect();
    check(15, nested.iter().map(|v| v.len()).sum::<usize>() == 190);
    check(16, nested[19].iter().sum::<u64>() == 171);
}

fn strings() {
    let s = format!("{}-{:x}-{:?}-{:>5}|", opaque(123), opaque(255u32), "ab", opaque(-7i64));
    check(20, s == "123-ff-\"ab\"-   -7|");
    let mut t = String::from("alloc");
    t.push_str(" on ");
    t += "RX";
    check(21, t == "alloc on RX" && t.len() == 11);
    check(22, t.to_uppercase() == "ALLOC ON RX");
    let words: Vec<&str> = t.split(' ').collect();
    check(23, words == ["alloc", "on", "RX"]);
    check(24, opaque(4_000_000_000u32).to_string() == "4000000000");
    check(25, format!("{:.3}", opaque(1.5f64) / 4.0) == "0.375");
    check(26, format!("{}", opaque(-0.1f32)) == "-0.1");
    let mut long = String::new();
    for i in 0..200 {
        long.push(char::from(b'a' + (i % 26) as u8));
    }
    // i = 198, 199: 'a' + 16, 'a' + 17
    check(27, long.len() == 200 && long.ends_with("qr"));
}

fn maps() {
    let mut m = BTreeMap::new();
    for i in 0..200u32 {
        m.insert(opaque(i * 37 % 200), i.to_string());
    }
    check(30, m.len() == 200);
    for k in (1..200).step_by(2) {
        m.remove(&k);
    }
    check(31, m.len() == 100 && m.keys().all(|k| k % 2 == 0));
    check(32, m.range(50..60).map(|(k, _)| *k).collect::<Vec<_>>() == [50, 52, 54, 56, 58]);
    check(33, m.get(&74).map(|s| s.as_str()) == Some("2"));
    let mut q = VecDeque::new();
    for i in 0..50 {
        if i % 2 == 0 { q.push_back(i) } else { q.push_front(i) }
    }
    check(34, q.front() == Some(&49) && q.back() == Some(&48) && q.len() == 50);
    let mut h: BinaryHeap<u32> = [5, 1, 8, 3, 9, 2].into_iter().map(opaque).collect();
    check(35, h.pop() == Some(9) && h.pop() == Some(8) && h.len() == 4);
}

fn shared() {
    let r = Rc::new(opaque(vec_of(5)));
    let r2 = Rc::clone(&r);
    check(40, Rc::strong_count(&r) == 2 && r2.iter().sum::<u32>() == 10);
    drop(r2);
    check(41, Rc::strong_count(&r) == 1);
    // Arc's counts are atomics: a compare-and-swap loop (movli / movco on
    // RXv2 and later, interrupts masked on RXv1).
    let a = Arc::new(opaque(7u32));
    let clones: Vec<Arc<u32>> = (0..10).map(|_| Arc::clone(&a)).collect();
    check(42, Arc::strong_count(&a) == 11 && clones.iter().map(|c| **c).sum::<u32>() == 70);
    drop(clones);
    check(43, Arc::strong_count(&a) == 1 && Arc::try_unwrap(a) == Ok(7));
}

fn vec_of(n: u32) -> Vec<u32> {
    (0..n).collect()
}

#[unsafe(no_mangle)]
fn main() -> ! {
    unsafe { HEAP.init(&raw mut HEAP_MEM as usize, HEAP_SIZE) };
    check(50, HEAP.used() == 0);

    for _ in 0..3 {
        boxes();
        vectors();
        strings();
        maps();
        shared();
        // Everything has been freed.
        check(51, HEAP.used() == 0);
    }

    unsafe { write_volatile(&raw mut finished, 1) };
    done()
}

// The end of the test (the board runner's breakpoint).
#[unsafe(no_mangle)]
#[inline(never)]
extern "C" fn done() -> ! {
    loop {
        rx_rt::interrupt::wait();
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe {
        failures += 1;
        first_failure = 999;
    }
    loop {}
}
