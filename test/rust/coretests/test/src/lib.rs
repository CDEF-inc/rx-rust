//! A stand-in for libtest that runs the `#[test]` functions of a test crate
//! on QEMU (bare metal, `panic = "abort"`, no threads or processes).
//!
//! rustc's test harness calls `test_main_env_args_abort` with the tests. They
//! run one after another and the results are printed on SCI0 in libtest's
//! format. A panic cannot unwind, so the panic handler records the result
//! of the running test (a pass for `#[should_panic]`), resets the stack and
//! jumps back into the loop with the next test; whatever the test allocated
//! or borrowed is leaked. The program also has the global allocator and the
//! start-up code, and sets `finished` at the end.

#![no_std]
#![feature(asm_experimental_arch)]
// The result variables keep the names the QEMU scripts read.
#![allow(non_upper_case_globals)]

extern crate alloc;

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::{format, string::ToString};
use core::alloc::{GlobalAlloc, Layout};
use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use core::ptr::{addr_of_mut, null_mut};
use std::println;
use std::process::Termination;

pub use std::process::ExitCode;
pub use TestFn::*;
pub use TestName::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestType {
    UnitTest,
    IntegrationTest,
    DocTest,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamePadding {
    PadNone,
    PadOnRight,
}

#[derive(Clone, Debug)]
pub enum TestName {
    StaticTestName(&'static str),
    DynTestName(String),
    AlignedTestName(Cow<'static, str>, NamePadding),
}

impl TestName {
    pub fn as_slice(&self) -> &str {
        match self {
            StaticTestName(s) => s,
            DynTestName(s) => s,
            AlignedTestName(s, _) => s,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShouldPanic {
    No,
    Yes,
    YesWithMessage(&'static str),
}

#[derive(Clone, Debug)]
pub struct TestDesc {
    pub name: TestName,
    pub ignore: bool,
    pub ignore_message: Option<&'static str>,
    pub source_file: &'static str,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub should_panic: ShouldPanic,
    pub compile_fail: bool,
    pub no_run: bool,
    pub test_type: TestType,
}

/// Benchmarks run once, as tests.
pub struct Bencher;

impl Bencher {
    pub fn iter<T, F: FnMut() -> T>(&mut self, mut f: F) {
        core::hint::black_box(f());
    }
}

pub enum TestFn {
    StaticTestFn(fn() -> Result<(), String>),
    StaticBenchFn(fn(&mut Bencher) -> Result<(), String>),
    StaticBenchAsTestFn(fn(&mut Bencher) -> Result<(), String>),
    DynTestFn(Arc<dyn Fn() -> Result<(), String> + Send + Sync>),
    DynBenchFn(Arc<dyn Fn(&mut Bencher) -> Result<(), String> + Send + Sync>),
    DynBenchAsTestFn(Arc<dyn Fn(&mut Bencher) -> Result<(), String> + Send + Sync>),
}

pub struct TestDescAndFn {
    pub desc: TestDesc,
    pub testfn: TestFn,
}

pub fn assert_test_result<T: Termination>(result: T) -> Result<(), String> {
    let code = result.report().to_i32();
    if code == 0 {
        Ok(())
    } else {
        Err(format!(
            "the test returned a termination value with a non-zero status code \
             ({code}) which indicates a failure"
        ))
    }
}

#[unsafe(no_mangle)]
static mut checks: u32 = 0;
#[unsafe(no_mangle)]
static mut failures: u32 = 0;
#[unsafe(no_mangle)]
static mut finished: u32 = 0;

static mut TESTS: &[&TestDescAndFn] = &[];
static mut NEXT: usize = 0;
/// The running test, or `usize::MAX` between tests.
static mut CURRENT: usize = usize::MAX;
static mut PASSED: usize = 0;
static mut IGNORED: usize = 0;
static mut FAILED: Vec<String> = Vec::new();

fn tests() -> &'static [&'static TestDescAndFn] {
    unsafe { *(&raw const TESTS) }
}

unsafe extern "C" {
    static __stack: u8;
}

/// The runner rustc's harness calls (`_abort` without `-Zpanic-abort-tests`
/// too: a test never unwinds here).
pub fn test_main_env_args(tests: &[&TestDescAndFn]) -> ExitCode {
    test_main_env_args_abort(tests)
}

pub fn test_main_env_args_abort(tests: &[&TestDescAndFn]) -> ExitCode {
    // The slice may be in main's stack frame, which a panic discards.
    let tests: Vec<&'static TestDescAndFn> =
        tests.iter().map(|&t| unsafe { &*(t as *const TestDescAndFn) }).collect();
    println!("\nrunning {} tests", tests.len());
    unsafe { TESTS = tests.leak() };
    run()
}

fn pass() {
    unsafe {
        PASSED += 1;
        checks += 1;
    }
}

fn fail(name: &str, why: String) {
    println!("---- {name} ----\n{why}");
    unsafe {
        checks += 1;
        failures += 1;
        (*addr_of_mut!(FAILED)).push(name.to_string());
    }
}

/// Runs the tests from `NEXT` on. It is entered again after each panic,
/// with the stack reset.
extern "C" fn run() -> ! {
    unsafe {
        while NEXT < tests().len() {
            let i = NEXT;
            NEXT = i + 1;
            let t = tests()[i];
            let name = t.desc.name.as_slice();
            if t.desc.ignore {
                match t.desc.ignore_message {
                    Some(m) => println!("test {name} ... ignored, {m}"),
                    None => println!("test {name} ... ignored"),
                }
                IGNORED += 1;
                continue;
            }
            println!("test {name} ...");
            CURRENT = i;
            let result = match &t.testfn {
                StaticTestFn(f) => f(),
                StaticBenchFn(f) | StaticBenchAsTestFn(f) => f(&mut Bencher),
                DynTestFn(f) => f(),
                DynBenchFn(f) | DynBenchAsTestFn(f) => f(&mut Bencher),
            };
            CURRENT = usize::MAX;
            match (result, t.desc.should_panic) {
                (Ok(()), ShouldPanic::No) => pass(),
                (Ok(()), _) => fail(name, "test did not panic as expected".into()),
                (Err(e), _) => fail(name, e),
            }
        }
        let (passed, ignored) = (PASSED, IGNORED);
        let failed = &*addr_of_mut!(FAILED);
        if !failed.is_empty() {
            println!("\nfailures:");
            for n in failed {
                println!("    {n}");
            }
        }
        println!(
            "\ntest result: {}. {passed} passed; {} failed; {ignored} ignored; 0 measured; \
             0 filtered out",
            if failed.is_empty() { "ok" } else { "FAILED" },
            failed.len()
        );
        finished = 1;
    }
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        let i = CURRENT;
        if i == usize::MAX {
            println!("panic outside a test: {info}");
            finished = 2;
            loop {}
        }
        CURRENT = usize::MAX;
        let t = tests()[i];
        let name = t.desc.name.as_slice();
        let msg = info.message().to_string();
        match t.desc.should_panic {
            ShouldPanic::Yes => pass(),
            ShouldPanic::YesWithMessage(want) if msg.contains(want) => pass(),
            ShouldPanic::YesWithMessage(want) => fail(
                name,
                format!("panic did not contain expected string\n  panic message: {msg:?}\n expected substring: {want:?}"),
            ),
            ShouldPanic::No => fail(name, format!("{info}")),
        }
        asm!("mov.l {sp}, r0", "jmp {run}",
             sp = in(reg) &raw const __stack, run = in(reg) run as *const () as usize,
             options(noreturn));
    }
}

// Start-up: the ELF is loaded by QEMU, so .data is in place and .bss is
// zero. rustc's `main` calls std's `start` lang item.
global_asm!(
    ".section .text.start, \"ax\"",
    ".global _start",
    "_start:",
    "mov.l #__stack, r0",
    "mov.l #0, r1",
    "mov.l #0, r2",
    "bsr.a _main",
    "1: bra.b 1b",
);

// A free list per power-of-two size class, on a static heap. Blocks of
// 2^c bytes are aligned to min(2^c, 4096); larger alignments are not
// reused.
const HEAP_SIZE: usize = 8 << 20;

#[repr(C, align(4096))]
struct HeapMem([u8; HEAP_SIZE]);

static mut HEAP: HeapMem = HeapMem([0; HEAP_SIZE]);
static mut TOP: usize = 0;
static mut FREE: [usize; 32] = [0; 32];

struct Heap;

fn class(l: Layout) -> usize {
    l.size().max(l.align()).max(8).next_power_of_two().trailing_zeros() as usize
}

unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let c = class(l);
        if c >= 32 {
            return null_mut();
        }
        unsafe {
            let head = FREE[c];
            if head != 0 && l.align() <= 4096 {
                FREE[c] = *(head as *const usize);
                return head as *mut u8;
            }
            let size = 1usize << c;
            let align = size.min(4096).max(l.align());
            let base = (&raw mut HEAP) as usize;
            let start = (base + TOP + align - 1) & !(align - 1);
            match start.checked_add(size) {
                Some(end) if end <= base + HEAP_SIZE => {
                    TOP = end - base;
                    start as *mut u8
                }
                _ => null_mut(),
            }
        }
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if l.align() > 4096 {
            return;
        }
        let c = class(l);
        unsafe {
            *(p as *mut usize) = FREE[c];
            FREE[c] = p as usize;
        }
    }
}

#[global_allocator]
static ALLOCATOR: Heap = Heap;
