/* GCC driver for the Rust staticlib in lib.rs (built for rx-none-elf). */

typedef long long i64;
typedef unsigned long long u64;
typedef unsigned int u32;

struct s3 { unsigned char c[3]; };
struct s8 { int a, b; };
struct s20 { int a[5]; };

int rs_add(int, int);
u64 rs_mul64(u64, u64);
u64 rs_muldiv128(u64, u64, u64);
double rs_f64(double, float);
int rs_s3(struct s3, int);
int rs_s8(struct s8, int);
struct s20 rs_ret20(int);
struct s8 rs_ret8(int);
int rs_call_c(int);
int rs_format(char *, unsigned, int, u32, u64, u64);
u32 rs_sort(int *, unsigned);
u32 rs_atomic(u32);
i64 rs_checked(i64, i64);

/* Called from Rust. */
int c_struct_mix(struct s3 a, struct s8 b, int c) {
  return a.c[0] + a.c[1] * 10 + a.c[2] * 100 + b.a * 3 - b.b + c * 1000;
}
struct s20 c_ret20(int a) { struct s20 s = {{a, 0, 0, 0, a * 7}}; return s; }
double c_double(double x) { return x * 0.5; }

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

static int streq(const char *a, const char *b, int n) {
  for (int i = 0; i < n; i++)
    if (a[i] != b[i])
      return 0;
  return b[n] == 0;
}

static volatile int ks[] = {0, 1, 300, -7, 123456};
#define N (int)(sizeof(ks) / sizeof(ks[0]))

int main(void) {
  for (int i = 0; i < N; i++) {
    int k = ks[i];
    check(rs_add(k, 5), k + 5);
    u64 m = rs_mul64((u64)k * 0x10001ULL, 0x123456789ULL);
    check((int)(m >> 32), (int)(((u64)k * 0x10001ULL * 0x123456789ULL) >> 32));
    /* a * b exceeds 64 bits, so this needs the 128-bit helpers. */
    u64 a = 0xfedcba9876543ULL + k, b = 0x100000ULL + (u32)k * 3ULL;
    u64 q = rs_muldiv128(a, b, b);
    check((int)q, (int)a);
    check((int)(q >> 32), (int)(a >> 32));
    double d = rs_f64(k * 0.25, 2.5f);
    check((int)(d * 4), (int)((k * 0.25 * 1.5 + 2.5 - 0.25) * 4));
    struct s3 x3 = {{(unsigned char)k, 9, (unsigned char)(k >> 8)}};
    check(rs_s3(x3, k), x3.c[0] + x3.c[2] * 256 + k);
    struct s8 x8 = {k, 11};
    check(rs_s8(x8, 3), k * 3 - 11);
    struct s20 r20 = rs_ret20(k);
    check(r20.a[0], k);
    check(r20.a[4], k * 10);
    struct s8 r8 = rs_ret8(k);
    check(r8.a, k);
    check(r8.b, -k);
    struct s3 y3 = {{(unsigned char)k, 7, (unsigned char)(k >> 8)}};
    struct s8 y8 = {k, 3};
    check(rs_call_c(k), c_struct_mix(y3, y8, k) + k * 7 + (int)(k * 0.5));
    check(rs_checked(k, 1000000000000LL) == -1, k > 9223372 || k < -9223372);
  }

  char buf[64];
  int n = rs_format(buf, sizeof buf, -42, 0xbeef, 1ULL << 36, 0); /* 2^100 */
  check(n, 42); /* 3 + 1 + 6 + 1 + 31 digits */
  check(streq(buf, "-42 0xbeef 1267650600228229401496703205376", n), 1);
  check(rs_format(buf, 5, 123456, 0, 0, 0), -1);

  int v[] = {5, -3, 99, 0, 42, -100, 7, 7, 13};
  u32 sum = rs_sort(v, 9);
  int sorted[] = {-100, -3, 0, 5, 7, 7, 13, 42, 99};
  u32 want = 0;
  for (int i = 0; i < 9; i++) {
    check(v[i], sorted[i]);
    want = want * 31 + ((u32)sorted[i] ^ (u32)i);
  }
  check((int)sum, (int)want);

  check((int)rs_atomic(5), 5);
  check((int)rs_atomic(10), 15);
  return failures;
}
