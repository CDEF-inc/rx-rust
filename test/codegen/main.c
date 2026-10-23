/* Driver for i32.ll: GCC computes the expected values, the llc-compiled
   functions produce the actual ones. Results are read from memory by gdb
   (see scripts/run-codegen-test.sh). */

typedef unsigned int u32;

int t_add(int, int), t_sub(int, int), t_mul(int, int), t_sdiv(int, int);
u32 t_udiv(u32, u32), t_urem(u32, u32);
int t_srem(int, int), t_and(int, int), t_or(int, int), t_xor(int, int);
int t_shl(int, int), t_ashr(int, int), t_shl7(int), t_ashr31(int);
u32 t_lshr(u32, u32), t_lshr20(u32);
int t_neg(int), t_not(int), t_imm(int), t_rsub(int);
int t_sext8(int), t_zext8(int), t_sext16(int), t_zext16(int);
u32 t_bswap(u32);
int t_smax(int, int), t_smin(int, int), t_abs(int);
u32 t_umax(u32, u32);
int t_popcount(u32), t_clz(u32);
int t_eq(int, int), t_ne(int, int), t_slt(int, int), t_sle(int, int);
int t_sgt(int, int), t_sge(int, int), t_lt100(int);
int t_ult(u32, u32), t_ule(u32, u32), t_ugt(u32, u32), t_uge(u32, u32);
int t_select(u32, u32, int, int);
int t_collatz(u32), t_fib(int), t_longbranch(int);
int t_global_sum(void), t_counter(int), t_narrow(void);
void t_store(void *, int);
int t_stack(int);
int t_args8(int, int, int, int, int, int, int, int);
int t_call_c(int, int);
signed char t_char(signed char, signed char);
unsigned short t_ushort(unsigned short);

int c_weighted6(int a, int b, int c, int d, int e, int f) {
  return a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f;
}

/* Exported results: number of checks, number of failures and the index of
   the first failing check (-1 if none). */
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

static int popcount(u32 x) { int n = 0; while (x) { n += x & 1; x >>= 1; } return n; }
static int clz(u32 x) { int n = 0; while (n < 32 && !(x & 0x80000000u)) { x <<= 1; n++; } return n; }
static int fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }
static int longbranch(int a) {
  static const u32 k[] = {
      305419896u,  19088743u,   287454020u,  1450744508u, 591751049u,
      286331153u,  1985229328u, 2023406814u, 1515870810u, 1431655765u,
      858993459u,  1717986918u, 2004318071u, 1145324612u, 572662306u,
      1999999999u, 1234567890u, 987654321u,  1357913579u, 246802468u,
      1122334455u, 1928374655u, 1029384756u, 1203948576u, 1111111111u,
      1222222222u};
  if (a == 0)
    return -1;
  u32 v = (u32)a;
  for (int i = 0; i < 26; i++)
    v = (i & 1) ? v + k[i] : v * k[i];
  return (int)v;
}
static int collatz(u32 n) { int s = 0; while (n > 1) { n = (n & 1) ? 3 * n + 1 : n >> 1; s++; } return s; }

static volatile int vals[] = {0, 1, -1, 7, -7, 100, -100, 12345, -98765,
                              0x7fffffff, (int)0x80000000, 0x12345678};
#define NV (int)(sizeof(vals) / sizeof(vals[0]))

int main(void) {
  for (int i = 0; i < NV; i++) {
    int a = vals[i];
    u32 ua = (u32)a;
    check(t_shl7(a), a << 7);
    check((int)t_lshr20(ua), (int)(ua >> 20));
    check(t_ashr31(a), a >> 31);
    check(t_neg(a), -a);
    check(t_not(a), ~a);
    check(t_imm(a), ((((a * 100) & 0xff00) | 0x12345678) ^ -129) - 70000);
    check(t_rsub(a), 1000 - a);
    check(t_sext8(a), (signed char)a);
    check(t_zext8(a), (unsigned char)a);
    check(t_sext16(a), (short)a);
    check(t_zext16(a), (unsigned short)a);
    check((int)t_bswap(ua), (int)__builtin_bswap32(ua));
    check(t_abs(a), a < 0 ? -a : a);
    check(t_popcount(ua), popcount(ua));
    check(t_clz(ua), clz(ua));
    check(t_lt100(a), a < 100);
    check(t_longbranch(a), longbranch(a));
    for (int j = 0; j < NV; j++) {
      int b = vals[j];
      u32 ub = (u32)b;
      check(t_add(a, b), a + b);
      check(t_sub(a, b), a - b);
      check(t_mul(a, b), a * b);
      if (b != 0 && !(a == (int)0x80000000 && b == -1)) {
        check(t_sdiv(a, b), a / b);
        check(t_srem(a, b), a % b);
      }
      if (ub != 0) {
        check((int)t_udiv(ua, ub), (int)(ua / ub));
        check((int)t_urem(ua, ub), (int)(ua % ub));
      }
      check(t_and(a, b), a & b);
      check(t_or(a, b), a | b);
      check(t_xor(a, b), a ^ b);
      int sh = b & 31;
      check(t_shl(a, sh), a << sh);
      check((int)t_lshr(ua, sh), (int)(ua >> sh));
      check(t_ashr(a, sh), a >> sh);
      check(t_smax(a, b), a > b ? a : b);
      check(t_smin(a, b), a < b ? a : b);
      check((int)t_umax(ua, ub), (int)(ua > ub ? ua : ub));
      check(t_eq(a, b), a == b);
      check(t_ne(a, b), a != b);
      check(t_slt(a, b), a < b);
      check(t_sle(a, b), a <= b);
      check(t_sgt(a, b), a > b);
      check(t_sge(a, b), a >= b);
      check(t_ult(ua, ub), ua < ub);
      check(t_ule(ua, ub), ua <= ub);
      check(t_ugt(ua, ub), ua > ub);
      check(t_uge(ua, ub), ua >= ub);
      check(t_select(ua, ub, a, b), ua < ub ? a : b);
    }
  }

  for (u32 n = 1; n < 30; n++)
    check(t_collatz(n), collatz(n));
  check(t_collatz(27), 111);
  for (int n = 0; n < 15; n++)
    check(t_fib(n), fib(n));

  check(t_global_sum(), -4);
  check(t_counter(5), 5);
  check(t_counter(-2), 3);
  check(t_narrow(), -1 + 127 + 128 - 32768 + 32767);

  static unsigned char buf[404];
  for (int i = 0; i < 404; i++)
    buf[i] = 0xaa;
  t_store(buf, 0x11223344);
  check(*(int *)buf, 0x11223344);
  check(buf[4], 0x44);
  check(buf[5], 0xaa);
  check(*(unsigned short *)(buf + 6), 0x3344);
  check(*(int *)(buf + 400), 0x11223344);

  for (int n = 1; n <= 16; n++) {
    int s = 0;
    for (int i = 0; i < n; i++)
      s += i * i;
    check(t_stack(n), s);
  }

  check(t_args8(1, 2, 3, 4, 5, 6, 7, 8), 1 + 4 + 9 + 16 + 25 + 36 + 49 + 64);
  check(t_args8(-1, 10, -100, 1000, -10000, 100000, -1000000, 10000000),
        -1 + 20 - 300 + 4000 - 50000 + 600000 - 7000000 + 80000000);
  for (int i = 0; i < NV; i++) {
    int x = vals[i], y = vals[(i + 3) % NV];
    int a = x * 3, b = y + 7;
    check(t_call_c(x, y), c_weighted6(x, y, a, b, 50, 60) + a + b);
  }

  check(t_char(100, 100), (signed char)200);
  check(t_char(-100, -100), (signed char)-200);
  check(t_ushort(30000), (unsigned short)90000);
  return failures;
}
