; Switches that become jump tables (dense case values) and ones that do
; not (sparse), and indirectbr. Linked with jt_main.c, which has the same
; switches in C and calls every function for every case value, the values
; around them and the default.
target triple = "rx-none-elf"

declare i32 @C_hit(i32)

; 0..9: a table indexed by x.
define i32 @L_dense(i32 %x) {
entry:
  switch i32 %x, label %def [
    i32 0, label %c0
    i32 1, label %c1
    i32 2, label %c2
    i32 3, label %c3
    i32 4, label %c4
    i32 5, label %c5
    i32 6, label %c6
    i32 7, label %c7
    i32 8, label %c8
    i32 9, label %c9
  ]
c0: ret i32 17
c1: ret i32 -4
c2: ret i32 1000
c3: ret i32 3
c4: ret i32 77
c5: ret i32 0
c6: ret i32 65536
c7: ret i32 -99999
c8: ret i32 8
c9: ret i32 123456789
def: ret i32 -1
}

; 100..111 with holes at 103 and 108 (they go to the default): a table
; indexed by x - 100. The cases call a function so that the blocks are not
; folded into a select.
define i32 @L_offset(i32 %x) {
entry:
  switch i32 %x, label %def [
    i32 100, label %a
    i32 101, label %b
    i32 102, label %c
    i32 104, label %d
    i32 105, label %a
    i32 106, label %b
    i32 107, label %c
    i32 109, label %d
    i32 110, label %e
    i32 111, label %e
  ]
a:
  %ra = call i32 @C_hit(i32 1)
  ret i32 %ra
b:
  %rb = call i32 @C_hit(i32 2)
  ret i32 %rb
c:
  %rc = call i32 @C_hit(i32 3)
  ret i32 %rc
d:
  %rd = call i32 @C_hit(i32 4)
  ret i32 %rd
e:
  %re = call i32 @C_hit(i32 5)
  ret i32 %re
def:
  ret i32 0
}

; -3..4: negative case values.
define i32 @L_neg(i32 %x) {
entry:
  switch i32 %x, label %def [
    i32 -3, label %m3
    i32 -2, label %m2
    i32 -1, label %m1
    i32 0, label %z
    i32 1, label %p1
    i32 2, label %p2
    i32 3, label %p3
    i32 4, label %p4
  ]
m3: ret i32 30
m2: ret i32 20
m1: ret i32 10
z:  ret i32 5
p1: ret i32 11
p2: ret i32 21
p3: ret i32 31
p4: ret i32 41
def: ret i32 -7
}

; A u8 (a Rust enum discriminant, say): the index is zero-extended.
define i32 @L_byte(i8 %x) {
entry:
  switch i8 %x, label %def [
    i8 250, label %a
    i8 251, label %b
    i8 252, label %c
    i8 253, label %d
    i8 254, label %e
    i8 255, label %f
  ]
a: ret i32 1
b: ret i32 2
c: ret i32 3
d: ret i32 4
e: ret i32 5
f: ret i32 6
def: ret i32 0
}

; Two tables in one function, and a value live across both.
define i32 @L_two(i32 %x, i32 %y) {
entry:
  switch i32 %x, label %mid [
    i32 0, label %x0
    i32 1, label %x1
    i32 2, label %x2
    i32 3, label %x3
    i32 4, label %x4
  ]
x0: br label %mid
x1: br label %mid
x2: br label %mid
x3: br label %mid
x4: br label %mid
mid:
  %v = phi i32 [ 100, %x0 ], [ 200, %x1 ], [ 300, %x2 ], [ 400, %x3 ],
               [ 500, %x4 ], [ 0, %entry ]
  switch i32 %y, label %out [
    i32 10, label %y0
    i32 11, label %y1
    i32 12, label %y2
    i32 13, label %y3
    i32 14, label %y4
  ]
y0: br label %out
y1: br label %out
y2: br label %out
y3: br label %out
y4: br label %out
out:
  %w = phi i32 [ 1, %y0 ], [ 2, %y1 ], [ 3, %y2 ], [ 4, %y3 ], [ 5, %y4 ],
               [ 0, %mid ]
  %r = add i32 %v, %w
  %s = add i32 %r, %x
  ret i32 %s
}

; Sparse: compares, no table.
define i32 @L_sparse(i32 %x) {
entry:
  switch i32 %x, label %def [
    i32 1, label %a
    i32 1000, label %b
    i32 100000, label %c
    i32 -5, label %d
  ]
a: ret i32 1
b: ret i32 2
c: ret i32 3
d: ret i32 4
def: ret i32 0
}

; A computed goto: indirectbr to a block address.
@targets = constant [3 x ptr] [ptr blockaddress(@L_goto, %t0),
                               ptr blockaddress(@L_goto, %t1),
                               ptr blockaddress(@L_goto, %t2)]
define i32 @L_goto(i32 %i) {
entry:
  %p = getelementptr [3 x ptr], ptr @targets, i32 0, i32 %i
  %t = load ptr, ptr %p
  indirectbr ptr %t, [label %t0, label %t1, label %t2]
t0: ret i32 7
t1: ret i32 8
t2: ret i32 9
}
