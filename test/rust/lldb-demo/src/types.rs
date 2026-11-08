//! Rust types for scripts/check-lldb.sh to show (#83): enums, `&str`,
//! slices, arrays, references, a closure and a generic struct. LLDB has no
//! Rust plugin: it reads them through its C type system, and check-lldb.sh
//! records what that gives.

use core::hint::black_box;

#[derive(Clone, Copy)]
pub enum Color {
    Red,
    Green = 5,
    Blue,
}

#[derive(Clone, Copy)]
pub enum Shape {
    Dot,
    Circle(u16),
    Rect { w: u8, h: u8 },
}

#[derive(Clone, Copy)]
pub struct Wrapper<T> {
    pub inner: T,
    pub tag: u8,
}

/// check-lldb.sh stops here and looks at `run`'s locals (`up`).
#[inline(never)]
fn seen() {
    black_box(());
}

#[inline(never)]
pub fn run() {
    let some: Option<u32> = black_box(Some(7));
    let none: Option<u32> = black_box(None);
    let ok: Result<u32, i8> = black_box(Ok(100));
    let err: Result<u32, i8> = black_box(Err(-3));
    let color = black_box(Color::Green);
    let dot = black_box(Shape::Dot);
    let circle = black_box(Shape::Circle(300));
    let rect = black_box(Shape::Rect { w: 4, h: 9 });
    let name: &str = black_box("rx-rust");
    let words: [u16; 3] = black_box([10, 20, 30]);
    let slice: &[u16] = black_box(&words[1..]);
    let bytes: [u8; 4] = black_box(*b"RX72");
    let point = super::Point { x: 11, y: -12 };
    let point_ref: &super::Point = black_box(&point);
    let wrapped = black_box(Wrapper { inner: -5i16, tag: 2 });
    let ch = black_box('λ');
    let flag = black_box(true);
    let tuple: (u8, i32) = black_box((1, -1));
    let k = black_box(3u32);
    let add_k = |x: u32| x + k;
    let sum = black_box(add_k(black_box(4)));
    seen();
    black_box((
        some, none, ok, err, color, dot, circle, rect, name, slice, bytes, point_ref,
        wrapped, ch, flag, tuple, &add_k, sum, Color::Red, Color::Blue,
    ));
}
