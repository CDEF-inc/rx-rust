/* Driver for atomic.ll. GCC computes the expected values with plain
   (non-atomic) C; the 8- and 16-bit operations must leave the neighbouring
   bytes of the containing word unchanged. */

typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

int cas32(u32 *, u32, u32), cas32_ok(u32 *, u32, u32);
int cas16(u16 *, u16, u16), cas8(u8 *, u8, u8);
u32 rmw_add(u32 *, u32), rmw_sub(u32 *, u32), rmw_and(u32 *, u32);
u32 rmw_or(u32 *, u32), rmw_xor(u32 *, u32), rmw_nand(u32 *, u32);
u32 rmw_xchg(u32 *, u32), rmw_max(u32 *, u32), rmw_min(u32 *, u32);
u32 rmw_umax(u32 *, u32), rmw_umin(u32 *, u32), rmw_uinc(u32 *, u32);
int rmw8_add(u8 *, u8), rmw8_min(u8 *, u8);
int rmw16_xchg(u16 *, u16), rmw16_umax(u16 *, u16);
int cas_loop(u32 *, int, int, int, int);

int checks, failures, first_failure = -1;
int got_value, want_value;

static void check(int got, int want) {
  if (got != want && failures++ == 0) {
    first_failure = checks;
    got_value = got;
    want_value = want;
  }
  checks++;
}

static volatile u32 word;
static union {
  u32 w;
  u16 h[2];
  u8 b[4];
} part;

typedef u32 (*rmw_fn)(u32 *, u32);

static u32 apply(int op, u32 a, u32 v) {
  switch (op) {
  case 0: return a + v;
  case 1: return a - v;
  case 2: return a & v;
  case 3: return a | v;
  case 4: return a ^ v;
  case 5: return ~(a & v);
  case 6: return v;
  case 7: return (int)a > (int)v ? a : v;
  case 8: return (int)a < (int)v ? a : v;
  case 9: return a > v ? a : v;
  case 10: return a < v ? a : v;
  default: return a >= v ? 0 : a + 1; /* uinc_wrap */
  }
}

int main(void) {
  /* 32-bit compare-and-swap */
  word = 100;
  check(cas32((u32 *)&word, 100, 200), 100);
  check((int)word, 200);
  check(cas32((u32 *)&word, 100, 300), 200);
  check((int)word, 200);
  check(cas32_ok((u32 *)&word, 200, -1), 1);
  check((int)word, -1);
  check(cas32_ok((u32 *)&word, 0, 5), 0);
  check((int)word, -1);

  /* 16- and 8-bit compare-and-swap, in every position of the word */
  for (int i = 0; i < 2; i++) {
    part.w = 0x12345678;
    u16 old = part.h[i], other = part.h[i ^ 1];
    check(cas16(&part.h[i], old, 0xbeef), 0x10000 | old);
    check(part.h[i], 0xbeef);
    check(part.h[i ^ 1], other);
    check(cas16(&part.h[i], old, 0x1111), 0xbeef);
    check(part.h[i], 0xbeef);
  }
  for (int i = 0; i < 4; i++) {
    part.w = 0x89abcdef;
    u8 old = part.b[i];
    u32 rest = part.w & ~(0xffu << (8 * i));
    check(cas8(&part.b[i], old, 0x5a), 0x100 | old);
    check(part.b[i], 0x5a);
    check((int)(part.w & ~(0xffu << (8 * i))), (int)rest);
    check(cas8(&part.b[i], old, 0x00), 0x5a);
  }

  /* 32-bit read-modify-write */
  static const rmw_fn fns[] = {rmw_add, rmw_sub, rmw_and,  rmw_or,
                               rmw_xor, rmw_nand, rmw_xchg, rmw_max,
                               rmw_min, rmw_umax, rmw_umin, rmw_uinc};
  static const u32 inits[] = {0, 1, 0x7fffffff, 0x80000000, 0xfffffffe, 12345};
  static const u32 vals[] = {0, 1, 5, 0x80000000, 0xffffffff, 12345, 0x0f0f};
  for (int op = 0; op < 12; op++)
    for (int i = 0; i < 6; i++)
      for (int j = 0; j < 7; j++) {
        word = inits[i];
        check((int)fns[op]((u32 *)&word, vals[j]), (int)inits[i]);
        check((int)word, (int)apply(op, inits[i], vals[j]));
      }

  /* 8- and 16-bit read-modify-write */
  for (int i = 0; i < 4; i++) {
    part.w = 0x7f80ff01;
    u8 old = part.b[i];
    u32 rest = part.w & ~(0xffu << (8 * i));
    check(rmw8_add(&part.b[i], 0x81), old);
    check(part.b[i], (u8)(old + 0x81));
    check((int)(part.w & ~(0xffu << (8 * i))), (int)rest);
    part.w = 0x7f80ff01;
    check(rmw8_min(&part.b[i], 0x10), (signed char)old);
    check(part.b[i], (signed char)old < 0x10 ? old : 0x10);
    check((int)(part.w & ~(0xffu << (8 * i))), (int)rest);
  }
  for (int i = 0; i < 2; i++) {
    part.w = 0xffff0001;
    u16 old = part.h[i], other = part.h[i ^ 1];
    check(rmw16_xchg(&part.h[i], 0x4321), old);
    check(part.h[i], 0x4321);
    check(part.h[i ^ 1], other);
    check(rmw16_umax(&part.h[i], 0x8000), 0x4321);
    check(part.h[i], 0x8000);
    check(part.h[i ^ 1], other);
  }

  /* a loop around compare-and-swap with several live values */
  word = 3;
  int acc = 0;
  u32 w = 3;
  for (int i = 0; i < 10; i++) {
    acc += (int)((w * 7u) ^ 0x55u);
    w += 11;
  }
  check(cas_loop((u32 *)&word, 10, 11, 7, 0x55), acc);
  check((int)word, (int)w);
  return failures;
}
