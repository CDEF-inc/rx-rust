/* Driver for m3.ll (build with -m64bit-doubles). GCC computes the expected
   values; doubles are compared bit for bit (both sides use libgcc soft-fp
   for double). Float arithmetic in GCC uses the RX FPU, LLVM calls
   __addsf3 & co.; the inputs avoid denormals so both round identically. */

#include <stdarg.h> /* from GCC; there is no libc */

typedef long long i64;
typedef unsigned long long u64;
typedef unsigned int u32;

i64 l_add(i64, i64), l_sub(i64, i64), l_mul(i64, i64), l_sdiv(i64, i64);
u64 l_udiv(u64, u64), l_urem(u64, u64);
i64 l_srem(i64, i64), l_and(i64, i64), l_or(i64, i64), l_xor(i64, i64);
i64 l_shl(i64, int), l_ashr(i64, int), l_shl40(i64), l_ashr3(i64);
u64 l_lshr(u64, int);
i64 l_neg(i64), l_addimm(i64), l_select(i64, i64), l_sext(int);
u64 l_zext(u32);
int l_trunc(i64), l_slt(i64, i64), l_ult(u64, u64), l_eq(i64, i64);
i64 p_2(int, i64), p_3(int, int, int, i64, int);
i64 p_5(int, int, int, int, int, i64), p_ll3(i64, i64, i64);
double d_add(double, double), d_sub(double, double), d_mul(double, double);
double d_div(double, double), d_neg(double), d_abs(double);
int d_lt(double, double), d_eq(double, double), d_ge(double, double);
int d_uno(double, double);
double d_fromi(int), d_fromu(u32), d_fromll(i64), d_fromf(float);
int d_toi(double);
i64 d_toll(double);
float d_tof(double);
double d_poly(double), d_pos(int, double, int);
double d_stack(int, int, int, double, double), d_select(double, double);
float f_add(float, float), f_sub(float, float), f_mul(float, float);
float f_div(float, float), f_fromi(int);
int f_lt(float, float), f_toi(float);
double f_mix(float, double);
double l_cross(i64, double);

/* Called from LLVM code. */
double c_mix(int a, i64 b, double c, float d, i64 e) {
  return a * 1000.0 + (double)b + c * 2.0 + d + (double)(e / 7);
}
i64 c_ret64(int a) { return (i64)a * 0x100000001LL - 5; }
double c_retd(int a) { return a / 8.0; }
double c_va(int n, ...) {
  va_list ap;
  va_start(ap, n);
  i64 x = va_arg(ap, i64);
  double y = va_arg(ap, double);
  int z = va_arg(ap, int);
  va_end(ap);
  return n * 0.5 + (double)x + y + z;
}

int checks, failures, first_failure = -1;
int got_value, want_value;

static void check64(u64 got, u64 want) {
  if (got != want && failures++ == 0) {
    first_failure = checks;
    got_value = (int)got;
    want_value = (int)want;
  }
  checks++;
}
static void check(int got, int want) { check64((u64)(i64)got, (u64)(i64)want); }
static u64 bits(double d) { u64 u; __builtin_memcpy(&u, &d, 8); return u; }
static u32 fbits(float f) { u32 u; __builtin_memcpy(&u, &f, 4); return u; }
static void checkd(double got, double want) { check64(bits(got), bits(want)); }
static void checkf(float got, float want) { check(fbits(got), fbits(want)); }

static volatile i64 v64[] = {0, 1, -1, 2, -3, 1000000007LL, 0xffffffffLL,
                             0x100000000LL, 0x123456789abcdef0LL,
                             -0x0fedcba987654321LL, 0x7fffffffffffffffLL,
                             (i64)0x8000000000000000ULL};
static volatile double vd[] = {0.0, -0.0, 1.0, -1.5, 3.14159265358979,
                               1e10, -2.5e-3, 123456.789, 7.0, -1e15};
static volatile float vf[] = {0.0f, 1.0f, -2.5f, 3.75f, 1e6f, -0.125f,
                              12345.678f};
#define N(a) (int)(sizeof(a) / sizeof(a[0]))

int main(void) {
  for (int i = 0; i < N(v64); i++) {
    i64 a = v64[i];
    u64 ua = (u64)a;
    check64(l_neg(a), -ua);
    check64(l_addimm(a), ua + 4294967297ULL);
    check64(l_shl40(a), ua << 40);
    check64(l_ashr3(a), a >> 3);
    check64(l_sext((int)a), (i64)(int)a);
    check64(l_zext((u32)a), (u64)(u32)a);
    check(l_trunc(a), (int)a);
    for (int s = 0; s < 64; s += 7) {
      check64(l_shl(a, s), ua << s);
      check64(l_lshr(ua, s), ua >> s);
      check64(l_ashr(a, s), a >> s);
    }
    for (int j = 0; j < N(v64); j++) {
      i64 b = v64[j];
      u64 ub = (u64)b;
      check64(l_add(a, b), ua + ub);
      check64(l_sub(a, b), ua - ub);
      check64(l_mul(a, b), ua * ub);
      if (b != 0 && !(a == (i64)0x8000000000000000ULL && b == -1)) {
        check64(l_sdiv(a, b), a / b);
        check64(l_srem(a, b), a % b);
      }
      if (ub != 0) {
        check64(l_udiv(ua, ub), ua / ub);
        check64(l_urem(ua, ub), ua % ub);
      }
      check64(l_and(a, b), a & b);
      check64(l_or(a, b), a | b);
      check64(l_xor(a, b), a ^ b);
      check(l_slt(a, b), a < b);
      check(l_ult(ua, ub), ua < ub);
      check(l_eq(a, b), a == b);
      check64(l_select(a, b), a > b ? a : b);
      check64(p_ll3(a, b, ua * 5), ua + ub * 2 + ua * 15);
    }
    check64(p_2(-7, a), ua - 7);
    check64(p_3(1, 2, 3, a, -11), ua * 3 - 11);
    check64(p_5(1, 2, 3, 4, 99, a), ua - 99);
  }

  for (int i = 0; i < N(vd); i++) {
    double a = vd[i];
    checkd(d_neg(a), -a);
    checkd(d_abs(a), a < 0 ? -a : (a == 0 ? 0.0 : a));
    checkd(d_poly(a), ((a * 1.5 + 2.25) * a - 0.125) / 3.0);
    check(d_toi(a * 1e-6), (int)(a * 1e-6));
    check64(d_toll(a), (i64)a);
    checkf(d_tof(a), (float)a);
    checkd(d_pos(-3, a, 17), a * -3.0 + 17.0);
    for (int j = 0; j < N(vd); j++) {
      double b = vd[j];
      checkd(d_add(a, b), a + b);
      checkd(d_sub(a, b), a - b);
      checkd(d_mul(a, b), a * b);
      if (b != 0)
        checkd(d_div(a, b), a / b);
      check(d_lt(a, b), a < b);
      check(d_eq(a, b), a == b);
      check(d_ge(a, b), a >= b);
      check(d_uno(a, b), 0);
      checkd(d_stack(1, 2, 3, a, b), a - b);
      checkd(d_select(a, b), a > b ? a : b);
    }
  }
  /* GCC enables -ffinite-math-only when it uses the RX FPU, so it would
     fold 0.0 / 0.0; build the NaN from its bits instead. */
  volatile u64 nan_bits = 0x7ff8000000000000ULL;
  u64 nb = nan_bits;
  double nan;
  __builtin_memcpy(&nan, &nb, 8);
  check(d_uno(nan, 1.0), 1);
  check(d_lt(nan, 1.0), 0);
  check(d_eq(nan, nan), 0);

  for (int i = 0; i < N(v64); i++) {
    i64 a = v64[i];
    checkd(d_fromi((int)a), (double)(int)a);
    checkd(d_fromu((u32)a), (double)(u32)a);
    checkd(d_fromll(a), (double)a);
  }

  for (int i = 0; i < N(vf); i++) {
    float a = vf[i];
    checkd(d_fromf(a), (double)a);
    check(f_toi(a), (int)a);
    checkf(f_fromi((int)(a * 3)), (float)(int)(a * 3));
    checkd(f_mix(a, vd[i]), (double)a * vd[i]);
    for (int j = 0; j < N(vf); j++) {
      float b = vf[j];
      checkf(f_add(a, b), a + b);
      checkf(f_sub(a, b), a - b);
      checkf(f_mul(a, b), a * b);
      if (b != 0)
        checkf(f_div(a, b), a / b);
      check(f_lt(a, b), a < b);
    }
  }

  for (int i = 0; i < N(v64); i++) {
    i64 x = v64[i] >> 20;
    double y = vd[i % N(vd)];
    float f = (float)y;
    int t = (int)x;
    i64 b = c_ret64(t);
    double c = c_retd(t);
    double want = c_mix(3, x, y, f, x * 2) + (double)b + c + c_va(3, b, c, t);
    checkd(l_cross(x, y), want);
  }
  return failures;
}
