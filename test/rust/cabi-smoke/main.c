/* GCC driver for lib.rs: char / short / bool arguments with undefined upper
   bits, as GCC passes them. The noinline callers below leave the sum of two
   chars unreduced (add r2, r1), sign-extend an unsigned char they load
   (mov.b), or pass an unsigned on as it is, so the Rust callee must extend
   the value itself. */

typedef unsigned int u32;
typedef long long i64;

struct s3 { unsigned char c[3]; };

u32 rs_u8(unsigned char);
int rs_i8(signed char);
u32 rs_u16(unsigned short);
int rs_i16(short);
u32 rs_bool(_Bool);
int rs_mix4(unsigned char, signed char, unsigned short, short);
int rs_after_i64(i64, unsigned char, short);
u32 rs_after_s3(struct s3, unsigned char);
unsigned char rs_ret_u8(u32);
signed char rs_ret_i8(u32);
unsigned short rs_ret_u16(u32);
short rs_ret_i16(u32);
int rs_call(u32 *, u32);

/* Called from Rust (rs_call). */
u32 c_u8(unsigned char a) { return a; }
int c_i8(signed char a) { return a; }
u32 c_u16(unsigned short a) { return a; }
int c_i16(short a) { return a; }
u32 c_bool(_Bool b) { return b ? 0x1234 : 0x5678; }
int c_mix4(unsigned char a, signed char b, unsigned short c, short d) {
  return a + b * 1000 + c * 3 - d * 7;
}
unsigned char c_ret_u8(u32 x) { return x; }
short c_ret_i16(u32 x) { return x; }

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

#define NOINLINE __attribute__((noinline, noclone))

/* The sum of two chars, not reduced to 8 / 16 bits. */
NOINLINE u32 sum_u8(unsigned char a, unsigned char b) { return rs_u8(a + b); }
NOINLINE int sum_i8(signed char a, signed char b) { return rs_i8(a + b); }
NOINLINE u32 sum_u16(unsigned short a, unsigned short b) {
  return rs_u16(a + b);
}
NOINLINE int sum_i16(short a, short b) { return rs_i16(a + b); }

/* The low bits of a wider value, passed on as it is. */
NOINLINE u32 fwd_u8(u32 x) { return rs_u8(x); }
NOINLINE int fwd_i8(u32 x) { return rs_i8(x); }
NOINLINE u32 fwd_u16(u32 x) { return rs_u16(x); }
NOINLINE int fwd_i16(u32 x) { return rs_i16(x); }
NOINLINE int fwd_mix4(u32 a, u32 b, u32 c, u32 d) {
  return rs_mix4(a, b, c, d);
}
NOINLINE int fwd_after_i64(i64 a, u32 b, u32 c) {
  return rs_after_i64(a, b, c);
}
NOINLINE u32 fwd_after_s3(struct s3 s, u32 b) { return rs_after_s3(s, b); }

/* An unsigned char loaded with a sign-extending mov.b. */
NOINLINE u32 load_u8(const unsigned char *p) { return rs_u8(*p); }
NOINLINE u32 load_u16(const unsigned short *p) { return rs_u16(*p); }

NOINLINE u32 bool_ne(u32 x) { return rs_bool(x != 0); }

volatile u32 seed = 3;
volatile unsigned char byte = 0x80;
volatile unsigned short half = 0x8001;

int main(void) {
  u32 k = seed;

  check(sum_u8(0xff, k - 1), 1);
  check(sum_u8(0x80, 0x80 + k), k);
  check(sum_i8(0x7f, k), (signed char)(0x7f + k));
  check(sum_i8(-128, -k), (signed char)(-128 - (int)k));
  check(sum_u16(0xffff, k), k - 1);
  check(sum_i16(0x7fff, k), (short)(0x7fff + k));

  check(fwd_u8(0x12345600 | k), k);
  check(fwd_u8(0xffffff80 + k), 0x80 + k);
  check(fwd_i8(0x12345680 + k), -128 + (int)k);
  check(fwd_i8(0xffffff00 + k), k);
  check(fwd_u16(0xabcd8000 + k), 0x8000 + k);
  check(fwd_i16(0x00018000 + k), -0x8000 + (int)k);
  check(fwd_i16(0xffff0000 + k), k);
  check(fwd_mix4(0xa5a5a5fe + k, 0x5a5a5a80 + k, 0xffff8000 + k, 0x0001ff00 + k),
         (unsigned char)(0xfe + k) + (signed char)(0x80 + k) * 1000 +
             (unsigned short)(0x8000 + k) * 3 - (short)(0xff00 + k) * 7);
  check(fwd_after_i64(0x1122334455667788LL, 0xcafe0080 | k, 0xbeef8000 | k),
        0x55667788 ^ ((0x80 | k) << 8) ^ (int)((0x8000 | k) << 16));
  check(fwd_after_s3((struct s3){{1, 2, k}}, 0x77777700 | (0xf0 + k)),
        1 + 20 + k * 100 + (0xf0 + k) * 1000);

  check(load_u8((const unsigned char *)&byte), 0x80);
  check(load_u16((const unsigned short *)&half), 0x8001);

  check(rs_bool(k == 3), 0x1234);
  check(bool_ne(k - 3), 0x5678);
  check(bool_ne(k << 30), 0x1234);

  /* Return values, extended by the Rust callee. */
  check(rs_ret_u8(0x1fc + k), 0xff);
  check(rs_ret_i8(0x17c + k), 0x7f);
  check(rs_ret_i8(0x27d + k), -128);
  check(rs_ret_u16(0x1fffc + k), 0xffff);
  check(rs_ret_i16(0x17ffd + k), -0x8000);

  /* rs_call makes these calls from Rust. */
  u32 want[9], got[12];
  int n = 0;
  want[n++] = c_u8((unsigned char)(k * 100));
  want[n++] = c_i8((signed char)(k * 50));
  want[n++] = c_u16((unsigned short)(k * 30000));
  want[n++] = c_i16((short)(k * -20000));
  want[n++] = c_bool((k & 2) != 0);
  want[n++] = c_bool((k & 4) != 0);
  want[n++] = c_mix4(k + 254, k * 43, k << 14, k << 13);
  want[n++] = c_ret_u8(k + 0x1fc);
  want[n++] = c_ret_i16(k + 0x17ffc);
  for (int i = 0; i < 12; i++)
    got[i] = 0xcccccccc;
  check(rs_call(got, k), n);
  for (int i = 0; i < n; i++)
    check((int)got[i], (int)want[i]);
  /* Literal values, in case GCC and Rust agree on something wrong. */
  check((int)got[0], 44);
  check((int)got[1], -106);
  check((int)got[4], 0x1234);
  check((int)got[5], 0x5678);
  check((int)got[7], 0xff);
  check((int)got[8], 0x7fff);
  return failures;
}
