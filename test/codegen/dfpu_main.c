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
double da_tree(double, double, double, double);
int da_chain_cmp(double, double), da_chain_toi(double, double);
int da_toi(double);
u32 da_tou(double);
double da_ext(float), da_ext_chain(float, double), d_ext(float);
double d_ext_f32only(float);
float da_trunc(double), da_trunc_chain(double, double), d_trunc(double);
void dm_add(double *, const double *, const double *);
double dm_off(const double *), dm_global(double), dm_stack(double, double);
void dm_store_off(double *, double, double);
long long dm_bits(const double *, double *);
void dm_volatile(double *, const double *), dm_unaligned(double *, const double *);
extern double dm_g;

static u32 fbits(float f) {
  union { float f; u32 u; } v = {f};
  return v.u;
}
static float ffrom_bits(u32 u) {
  union { u32 u; float f; } v = {u};
  return v.f;
}

/* Called by da_call. */
double ext(double x) { return x * 2.0 + 1.0; }
/* Called by dm_stack with its stack array: element 2 += 1. */
void dm_use(double *s) { s[2] += 1.0; }
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
      check(da_chain_cmp(a, b), a * b + a < b);
      /* within float range, so that dtof neither overflows nor flushes */
      double s = a + b;
      if (s == 0.0 || (s > -3e38 && s < 3e38 && (s > 1e-37 || s < -1e-37)))
        check((int)fbits(da_trunc_chain(a, b)), (int)fbits((float)s));
      for (unsigned k = 0; k < NV; k++) {
        double c = vals[k], d = 0.25 * (double)(k + 1);
        double t = a * b - c * d, u = (a + b) * (c - d);
        if (u != 0.0)
          check_d(da_tree(a, b, c, d), t / u + t);
      }
    }

  /* conversions: dtoi / dtou truncate; ftod / dtof are exact or round to
     nearest; the IEEE functions agree on these values */
  static const double cv[] = {0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 3.25, -7.125,
                              123456.789, -123456.789, 1e-15, -3e-100,
                              2147483647.0, -2147483648.0, 4294967295.0,
                              0.1, 1e30, -2.5e15};
  for (unsigned i = 0; i < sizeof cv / sizeof cv[0]; i++) {
    double x = cv[i];
    if (x > -2147483649.0 && x < 2147483648.0) {
      check(da_toi(x), (int)x);
      check(da_chain_toi(x, 1.0), (int)x);
    }
    if (x > -1.0 && x < 4294967296.0)
      check((int)da_tou(x), (int)(u32)x);
    if (x == 0.0 || (x > -3e38 && x < 3e38 && (x > 1e-37 || x < -1e-37))) {
      check((int)fbits(da_trunc(x)), (int)fbits((float)x));
      check((int)fbits(d_trunc(x)), (int)fbits((float)x));
    }
    float f = (float)x;
    if (f == 0.0f || f > 1e-37f || f < -1e-37f) {
      check_d(da_ext(f), (double)f);
      check_d(d_ext(f), (double)f);
      check_d(da_ext_chain(f, 3.0), (double)f * 3.0);
    }
  }
  /* denormals: flushed by the DFPU, kept by the IEEE functions */
  float fmin = ffrom_bits(1);
  check_d(da_ext(fmin), 0.0);
  check_d(d_ext(fmin), (double)fmin);
  check_d(d_ext_f32only(fmin), (double)fmin);
  check_d(d_ext_f32only(1.5f), 1.5);
  check((int)fbits(d_trunc(1e-40)), (int)fbits((float)1e-40));
  check((int)fbits(da_trunc(1e-40)), 0);

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

  /* dmov.d loads and stores, at word offset 0 and 1 of a word buffer, so
     on 8- and on 4-but-not-8-aligned addresses either way */
  static u32 w[2 + 2 * 300];
  for (unsigned o = 0; o < 2; o++) {
    double *p = (double *)&w[o];
    for (unsigned i = 0; i < NV; i++) {
      double a = vals[i], b = vals[(i + 5) % NV];
      p[0] = a;
      p[1] = b;
      p[2] = 99.0;
      dm_add(&p[2], &p[0], &p[1]);
      check_d(p[2], a + b);
      check_d(p[0], a);
      check_d(p[1], b);
      p[1] = a;
      p[128] = b;
      check_d(dm_off(&p[0]), a * b);
      dm_store_off(&p[0], a, b);
      /* byte 1020: words 255 and 256 from p */
      u32 *q = &w[o + 255];
      check_d(from_bits((u64)q[1] << 32 | q[0]), a - b);
      dm_g = a;
      check_d(dm_global(b), a * b);
      check_d(dm_g, a * b);
      p[3] = 0.0;
      long long ab = dm_bits(&p[0], &p[3]);
      check((int)ab, (int)bits(a));
      check((int)(ab >> 32), (int)(bits(a) >> 32));
      check_d(p[3], a + a);
      dm_volatile(&p[4], &p[0]);
      check_d(p[4], a + a);
      /* dm_unaligned reads and writes the double at byte 2 */
      unsigned char *src = (unsigned char *)&p[6], *dst = (unsigned char *)&p[9];
      for (unsigned k = 0; k < 8; k++) {
        src[2 + k] = (unsigned char)(bits(a) >> 8 * k);
        dst[2 + k] = 0;
      }
      dm_unaligned((double *)dst, (const double *)src);
      u64 out = 0;
      for (unsigned k = 0; k < 8; k++)
        out |= (u64)dst[2 + k] << 8 * k;
      check_d(from_bits(out), a + a);
    }
  }
  for (unsigned i = 0; i < NV; i++) {
    double a = vals[i], b = vals[(i + 3) % NV];
    double s = a + b + 1.0;
    check_d(dm_stack(a, b), s * s);
  }
  return failures;
}
