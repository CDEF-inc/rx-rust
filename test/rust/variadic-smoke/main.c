/* GCC driver for the variadic functions in lib.rs. GCC's own va_arg (the
   collect function below) gives the expected values. */

#include <stdarg.h>
#include <stddef.h>

typedef long long i64;
typedef unsigned long long u64;
typedef unsigned int u32;

struct s3 { unsigned char c[3]; };
struct s8 { int a, b; };
struct s4 { unsigned char c[4]; };
struct s1 { unsigned char c; };

int rs_collect(u32 *, const char *, ...);
int rs_collect_named(int, i64, int, int, u32 *, const char *, ...);
int rs_collect_twice(u32 *, const char *, ...);
int rs_vcollect(u32 *, const char *, va_list);
int rs_sum(int, ...);
int rs_fwd(int, ...);
int rs_call(u32 *, int);
extern const int RS_X;

/* Called from Rust. */
int c_sum(int n, ...) {
  va_list ap;
  va_start(ap, n);
  int s = 0;
  for (int i = 0; i < n; i++)
    s += va_arg(ap, int);
  va_end(ap);
  return s;
}

static u64 dbits(double d) {
  union { double d; u64 u; } v = {d};
  return v.u;
}

i64 c_mixed(int a, int b, int c, ...) {
  va_list ap;
  va_start(ap, c);
  i64 q = va_arg(ap, i64);
  int i = va_arg(ap, int);
  double d = va_arg(ap, double);
  const int *p = va_arg(ap, const int *);
  va_end(ap);
  return q * 3 + i + (i64)(dbits(d) >> 16) + *p + a + b * 10 + c * 100;
}

double c_dsum(int n, ...) {
  va_list ap;
  va_start(ap, n);
  double s = 0;
  for (int i = 0; i < n; i++)
    s += va_arg(ap, double) * (i + 1);
  va_end(ap);
  return s;
}

int c_structs(int n, ...) {
  va_list ap;
  va_start(ap, n);
  struct s8 x = va_arg(ap, struct s8);
  struct s3 y = va_arg(ap, struct s3);
  int z = va_arg(ap, int);
  va_end(ap);
  return n + x.a * 3 - x.b * 5 + y.c[0] + y.c[1] * 10 + y.c[2] * 100 + z * 1000;
}

static int s4val(struct s4 s) {
  return s.c[0] + s.c[1] * 10 + s.c[2] * 100 + s.c[3] * 1000;
}

/* Structures of chars, packed on the stack at their own alignment. */
int c_packed(int n, ...) {
  va_list ap;
  va_start(ap, n);
  struct s3 x = va_arg(ap, struct s3);
  struct s4 y = va_arg(ap, struct s4);
  struct s1 z = va_arg(ap, struct s1);
  int i = va_arg(ap, int);
  va_end(ap);
  return n + x.c[0] + x.c[1] * 3 + x.c[2] * 5 + s4val(y) * 7 + z.c * 11 +
         i * 100000;
}

/* The last named argument is on the stack too, after a. */
int c_last(int n, struct s3 a, struct s4 b, ...) {
  va_list ap;
  va_start(ap, b);
  int i = va_arg(ap, int);
  va_end(ap);
  return n + a.c[0] + a.c[1] * 3 + a.c[2] * 5 + s4val(b) * 7 + i * 100000;
}

int c_vsum(int n, va_list ap) {
  int s = 0;
  for (int i = 0; i < n; i++)
    s += va_arg(ap, int) * (i + 1);
  return s;
}

int c_collect(u32 *out, const char *fmt, ...) {
  va_list ap;
  va_start(ap, fmt);
  int n = rs_vcollect(out, fmt, ap);
  va_end(ap);
  return n;
}

/* lib.rs's collect, with GCC's va_arg. */
static int collect(u32 *out, const char *fmt, va_list ap) {
  int n = 0;
  for (const char *f = fmt; *f; f++) {
    u64 w;
    switch (*f) {
    case 'i': out[n++] = va_arg(ap, int); break;
    case 'u': out[n++] = va_arg(ap, unsigned); break;
    case 'l': out[n++] = va_arg(ap, long); break;
    case 'q': w = va_arg(ap, i64); goto put64;
    case 'Q': w = va_arg(ap, u64); goto put64;
    case 'z': out[n++] = va_arg(ap, size_t); break;
    case 'Z': out[n++] = va_arg(ap, ptrdiff_t); break;
    case 'd': w = dbits(va_arg(ap, double)); goto put64;
    case 'p': out[n++] = (u32)va_arg(ap, void *); break;
    case 's': out[n++] = (unsigned char)*va_arg(ap, const char *); break;
    default: out[n++] = 0xdead0000u | (unsigned char)*f; break;
    put64:
      out[n++] = (u32)w;
      out[n++] = (u32)(w >> 32);
    }
  }
  return n;
}

static int c_ref(u32 *out, const char *fmt, ...) {
  va_list ap;
  va_start(ap, fmt);
  int n = collect(out, fmt, ap);
  va_end(ap);
  return n;
}

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

static u32 want[40], got[80];

/* got[off..off+n] == want[0..n] */
static void check_words(int off, int n) {
  for (int i = 0; i < n; i++)
    check((int)got[off + i], (int)want[i]);
}

static void clear(void) {
  for (int i = 0; i < 80; i++)
    got[i] = 0xcccccccc;
}

/* Each Rust reader against GCC's, with the same arguments. */
#define CASE(fmt, ...)                                                   \
  do {                                                                   \
    int n = c_ref(want, fmt, __VA_ARGS__);                               \
    clear();                                                             \
    check(rs_collect(got, fmt, __VA_ARGS__), n);                         \
    check_words(0, n);                                                   \
    clear();                                                             \
    check(c_collect(got, fmt, __VA_ARGS__), n);                          \
    check_words(0, n);                                                   \
    clear();                                                             \
    check(rs_collect_twice(got, fmt, __VA_ARGS__), 2 * n);               \
    check_words(0, n);                                                   \
    check_words(n, n);                                                   \
    clear();                                                             \
    check(rs_collect_named(-3, 0x1122334455667788LL, k, 4 * k, got, fmt, \
                           __VA_ARGS__),                                 \
          n + 5);                                                        \
    check((int)got[0], -3);                                              \
    check((int)got[1], 0x55667788);                                      \
    check((int)got[2], 0x11223344);                                      \
    check((int)got[3], k);                                               \
    check((int)got[4], 4 * k);                                           \
    check_words(5, n);                                                   \
  } while (0)

volatile int seed = 3;

int main(void) {
  int k = seed;
  int x = 0x7f;
  const char *str = "Rust";

  check(rs_collect(got, ""), 0);
  check(rs_collect_named(1, 2, 3, 4, got, ""), 5);
  CASE("i", k);
  CASE("iiiiiiii", 1, 2, 3, 4, 5, 6, 7, k);
  CASE("q", 0x123456789LL);
  CASE("iq", k, -2LL);
  CASE("qid", 0x123456789LL, k, 1.0 / 3);
  CASE("d", 2.5);
  CASE("dd", -0.0, 1.0 / 3);
  CASE("idq", k, 2.5, -2LL);
  CASE("pszZu", &x, str, (size_t)k, (ptrdiff_t)-k, 0xfffffff0u);
  CASE("Ql", 0xfedcba9876543210ULL, -7L);
  CASE("iqdipQdiu", k, -1LL, 1e300, 5, &x, 0x8000000000000001ULL, -1.5, -k,
       7u);

  /* Literal values, in case GCC and Rust agree on something wrong. */
  clear();
  check(rs_collect(got, "iqd", 0x1234, 0x123456789LL, 2.5), 5);
  check((int)got[0], 0x1234);
  check((int)got[1], 0x23456789);
  check((int)got[2], 1);
  check((int)got[3], 0);
  check((int)got[4], 0x40040000);

  check(rs_sum(0), 0);
  check(rs_sum(1, k), k);
  check(rs_sum(3, 1, 2, 3), 6);
  check(rs_sum(8, 1, 2, 3, 4, 5, 6, 7, k), 28 + k);
  check(rs_fwd(4, 10, 20, 30, k), 10000 + 20 + 60 + 3 * k);
  check(rs_fwd(1, -5), -5000);

  /* rs_call makes these calls from Rust. */
  int n = 0;
  want[n++] = c_sum(0);
  want[n++] = c_sum(1, k);
  want[n++] = c_sum(8, 1, 2, 3, 4, 5, 6, 7, k);
  u64 m = c_mixed(1, 2, k, 0x123456789LL, -5, 2.5, &RS_X);
  want[n++] = (u32)m;
  want[n++] = (u32)(m >> 32);
  u64 d = dbits(c_dsum(3, 1.5, 1.0 / 3, (double)k));
  want[n++] = (u32)d;
  want[n++] = (u32)(d >> 32);
  want[n++] = c_structs(2, (struct s8){k, -k}, (struct s3){{1, 2, 3}}, k);
  want[n++] = c_structs(3, (struct s8){7, k},
                        (struct s3){{9, 8, (unsigned char)k}}, -1);
  struct s4 s4 = {{4, 5, 6, (unsigned char)k}};
  want[n++] = c_packed(1, (struct s3){{1, 2, 3}}, s4, (struct s1){7}, k);
  want[n++] = c_last(2, (struct s3){{(unsigned char)k, 2, 3}}, s4, -k);
  want[n++] = 60 + k;
  clear();
  check(rs_call(got, k), n);
  check_words(0, n);
  return failures;
}
