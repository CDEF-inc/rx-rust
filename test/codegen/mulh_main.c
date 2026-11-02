/* Driver for mulh.ll: division by constants, multiply-high, multiplies with
   an overflow check and 64-bit multiplies, against real divisions (the
   divisors are volatile, so GCC divides) and 64-bit products. */

typedef unsigned int u32;
typedef int i32;
typedef unsigned long long u64;
typedef long long i64;

u32 L_udiv3(u32), L_udiv7(u32), L_udiv10(u32), L_urem10(u32);
u32 L_udiv1000(u32), L_urem10000(u32), L_udiv641(u32), L_udivbig(u32);
u32 L_udiv16(u32), L_udivh10(u32), L_udivvar(u32, u32);
i32 L_sdiv3(i32), L_sdiv7(i32), L_sdiv10(i32), L_srem10(i32);
i32 L_sdivm3(i32), L_sremm7(i32), L_sdiv1000(i32);
u32 L_mulhu(u32, u32);
i32 L_mulhs(i32, i32);
int L_umulo(u32, u32, u32 *), L_smulo(i32, i32, i32 *);
u64 L_mulzz(u32, u32);
i64 L_mulss(i32, i32);
u64 L_mul64(u64, u64), L_udiv64x10(u64), L_urem64x3(u64);

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

static void check64(u64 got, u64 want) {
  check((int)got, (int)want);
  check((int)(got >> 32), (int)(want >> 32));
}

static volatile u32 d3 = 3, d7 = 7, d10 = 10, d16 = 16, d1000 = 1000;
static volatile u32 d10000 = 10000, d641 = 641, dbig = 2147483649u;
static volatile i32 dm3 = -3, dm7 = -7;
static volatile u64 d64_10 = 10, d64_3 = 3;

static const u32 edges[] = {
    0, 1, 2, 3, 6, 7, 9, 10, 11, 99, 100, 101, 640, 641, 642, 999, 1000,
    1001, 9999, 10000, 10001, 65535, 65536, 0x7ffffffe, 0x7fffffff,
    0x80000000, 0x80000001, 0x80000002, 0xfffffff6, 0xfffffffd, 0xfffffffe,
    0xffffffff};
#define NEDGES (int)(sizeof(edges) / sizeof(edges[0]))

static void one(u32 a, u32 b) {
  i32 s = (i32)a, t = (i32)b;

  check(L_udiv3(a), a / d3);
  check(L_udiv7(a), a / d7);
  check(L_udiv10(a), a / d10);
  check(L_urem10(a), a % d10);
  check(L_udiv1000(a), a / d1000);
  check(L_urem10000(a), a % d10000);
  check(L_udiv641(a), a / d641);
  check(L_udivbig(a), a / dbig);
  check(L_udiv16(a), a / d16);
  check(L_udivh10(a), (unsigned short)a / (unsigned short)d10);
  if (b)
    check(L_udivvar(a, b), a / b);

  /* INT_MIN / -1 is undefined; the divisors here are never -1. */
  check(L_sdiv3(s), s / (i32)d3);
  check(L_sdiv7(s), s / (i32)d7);
  check(L_sdiv10(s), s / (i32)d10);
  check(L_srem10(s), s % (i32)d10);
  check(L_sdivm3(s), s / dm3);
  check(L_sremm7(s), s % dm7);
  check(L_sdiv1000(s), s / (i32)d1000);

  u64 pu = (u64)a * b;
  i64 ps = (i64)s * t;
  check(L_mulhu(a, b), (u32)(pu >> 32));
  check(L_mulhs(s, t), (i32)(ps >> 32));
  u32 vu;
  check(L_umulo(a, b, &vu), (pu >> 32) != 0);
  check(vu, (u32)pu);
  i32 vs;
  check(L_smulo(s, t, &vs), ps != (i64)(i32)ps);
  check(vs, (i32)ps);
  check64(L_mulzz(a, b), pu);
  check64(L_mulss(s, t), (u64)ps);

  u64 x = (u64)a << 32 | b, y = (u64)b * 0x9e3779b9u + a;
  check64(L_mul64(x, y), x * y);
  check64(L_udiv64x10(x), x / d64_10);
  check64(L_urem64x3(x), x % d64_3);
  check64(L_udiv64x10(a), a / d64_10);
}

int main(void) {
  for (int i = 0; i < NEDGES; i++)
    for (int j = 0; j < NEDGES; j += 3)
      one(edges[i], edges[j]);
  u32 r = 12345;
  for (int i = 0; i < 300; i++) {
    u32 a = r = r * 1103515245u + 12345;
    u32 b = r = r * 1103515245u + 12345;
    /* Mix in small values, where divisions by constants change most. */
    one(i & 1 ? a : a >> (i % 31), i & 2 ? b : b >> (i % 29));
  }
  return failures;
}
