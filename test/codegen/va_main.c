/* Driver for va.ll: variadic functions defined by LLVM, called by GCC. */

#include <stdarg.h>

int va_sum(int, ...), va_twice(int, ...), va_fwd(int, ...);
int va_frame(int, ...);
long long va_mixed(int, int, int, ...);
double va_dsum(int, ...);

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

static void check_ll(long long got, long long want) {
  check((int)got, (int)want);
  check((int)(got >> 32), (int)(want >> 32));
}

/* Called by va_fwd with LLVM's va_list. */
int c_vsum(int n, va_list ap) {
  int s = 0;
  for (int i = 0; i < n; i++)
    s += va_arg(ap, int) * (i + 1);
  return s;
}

static unsigned long long dbits(double d) {
  union { double d; unsigned long long u; } v = {d};
  return v.u;
}

volatile int seed = 3;

int main(void) {
  int k = seed;
  check(va_sum(0), 0);
  check(va_sum(1, k), k);
  check(va_sum(3, 1, 2, 3), 6);
  check(va_sum(8, 1, 2, 3, 4, 5, 6, 7, k), 28 + k);
  int x = 0x55;
  check_ll(va_mixed(1, 2, k, 0x123456789LL, -5, 2.5, &x),
           0x123456789LL - 5 + (long long)(dbits(2.5) >> 32) + 0x55 + 3 + k);
  check_ll(va_mixed(-1, -2, -3, -1LL, 7, -0.0, &x),
           -1LL + 7 + (long long)(dbits(-0.0) >> 32) + 0x55 - 6);
  double d = va_dsum(3, 1.5, 2.25, (double)k);
  check((int)dbits(d), (int)dbits(1.5 + 2.25 + k));
  check((int)(dbits(d) >> 32), (int)(dbits(1.5 + 2.25 + k) >> 32));
  check(va_twice(2, 7, 11), 7 + 11 + 7000);
  check(va_fwd(4, 10, 20, 30, k), 10 + 40 + 90 + 4 * k);
  check(va_frame(5, 100, 30), 100 - 30 + 5);
  return failures != 0;
}
