/* Driver for dfpu.ll (build with -m64bit-doubles, run with MCPU=rxv3 on a
   core with the DFPU). GCC computes double with libgcc soft-fp (IEEE), so
   for ordinary values the DFPU results of the fast-math functions are
   compared bit for bit; the soft-float functions are checked alongside. */

typedef unsigned int u32;
typedef unsigned long long u64;

double da_add(double, double), da_sub(double, double);
double da_mul(double, double), da_div(double, double), da_sqrt(double);
double da_chain(double, double, double), da_max(double, double);
int da_olt(double, double), da_ole(double, double), da_ogt(double, double);
int da_oge(double, double), da_oeq(double, double), da_une(double, double);
int da_branch(double, double);
double d_fromi(int), d_fromu(u32), d_add(double, double);
double da_add_ieee(double, double);
int da_olt_ieee(double, double), d_olt(double, double), d_uno(double, double);
double d_fromll(long long), da_call(double, double);
int d_toi(double);

/* Called by da_call. */
double ext(double x) { return x * 2.0 + 1.0; }
/* d_sqrt calls sqrt; it is not checked (no libm here). */
double sqrt(double x) { return x; }

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

static u64 bits(double d) {
  union { double d; u64 u; } v = {d};
  return v.u;
}
static double from_bits(u64 u) {
  union { u64 u; double d; } v = {u};
  return v.d;
}
static void check_d(double got, double want) {
  check((int)bits(got), (int)bits(want));
  check((int)(bits(got) >> 32), (int)(bits(want) >> 32));
}

/* No sums, products or quotients of these overflow or become denormal. */
static const double vals[] = {0.0, -0.0, 1.0, -1.0, 0.5, 3.25, -7.125,
                              1e100, -3e-100, 123456.789, 1e-15, -2.5e15};
#define NV (sizeof vals / sizeof vals[0])

int main(void) {
  for (unsigned i = 0; i < NV; i++)
    for (unsigned j = 0; j < NV; j++) {
      double a = vals[i], b = vals[j];
      check_d(da_add(a, b), a + b);
      check_d(da_sub(a, b), a - b);
      check_d(da_mul(a, b), a * b);
      if (b != 0.0)
        check_d(da_div(a, b), a / b);
      check(da_olt(a, b), a < b);
      check(da_ole(a, b), a <= b);
      check(da_ogt(a, b), a > b);
      check(da_oge(a, b), a >= b);
      check(da_oeq(a, b), a == b);
      check(da_une(a, b), a != b);
      check_d(da_max(a, b), a > b ? a : b);
      check(da_branch(a, b), a < b ? -1 : 1);
      check_d(d_add(a, b), a + b);
      check_d(da_add_ieee(a, b), a + b);
      check(da_olt_ieee(a, b), a < b);
      check(d_olt(a, b), a < b);
      check(d_uno(a, b), 0);
      if (a != 0.0)
        check_d(da_chain(a, b, 0.75), (a * b - 0.75) / a);
      check_d(da_call(a, b), ext(a + b) * a);
    }

  /* sqrt: exact squares, and sqrt(2) */
  static const double sq[][2] = {
    {0.0, 0.0}, {1.0, 1.0}, {4.0, 2.0}, {2.25, 1.5}, {0.0625, 0.25},
    {1099511627776.0, 1048576.0},
  };
  for (unsigned i = 0; i < sizeof sq / sizeof sq[0]; i++)
    check_d(da_sqrt(sq[i][0]), sq[i][1]);
  check_d(da_sqrt(2.0), from_bits(0x3ff6a09e667f3bcdULL));

  /* integer to double (DFPU) and back (soft-float) */
  static const int ints[] = {0, 1, -1, 7, 16777217, -16777217, 0x7fffffff,
                             (int)0x80000000};
  for (unsigned i = 0; i < 8; i++) {
    check_d(d_fromi(ints[i]), (double)ints[i]);
    check_d(d_fromu((u32)ints[i]), (double)(u32)ints[i]);
    check(d_toi((double)ints[i]), ints[i]);
  }
  check_d(d_fromll(0x123456789abcdefLL), (double)0x123456789abcdefLL);

  /* Denormals: the fast-math functions may flush them (DPSW.DDN = 1 after
     reset), the IEEE ones keep them. */
  double dmin = from_bits(1);
  check_d(da_add(dmin, 1.0), 1.0);
  check_d(da_add(dmin, dmin), 0.0);
  check_d(da_add_ieee(dmin, dmin), from_bits(2));
  check(da_olt_ieee(0.0, dmin), 1);
  check(da_olt(0.0, dmin), 0);
  return failures;
}
