/* Driver for fpu.ll (build with -m64bit-doubles). GCC uses the RX FPU for
   float, so for ordinary values its results are the IEEE ones and are
   compared bit for bit. GCC assumes -ffinite-math-only with the FPU, so the
   expected results for NaN and infinity inputs are written out by hand. */

typedef unsigned int u32;

float fa_add(float, float), fa_sub(float, float), fa_mul(float, float);
float fa_div(float, float), fa_poly(float), fa_max(float, float);
int fa_olt(float, float), fa_ole(float, float), fa_ogt(float, float);
int fa_oge(float, float), fa_oeq(float, float), fa_une(float, float);
int fa_branch(float, float);
float f_add(float, float), f_mul(float, float), f_div(float, float);
int f_olt(float, float), f_ult(float, float), f_oeq(float, float);
int f_une(float, float), f_uno(float, float);
float f_sel(float, float), f_neg(float), f_abs(float);
float f_copysign(float, float), f_fromi(int), f_fromu(u32);
int f_toi(float);
u32 f_tou(float);
double f_ext(float);
float f_trunc(double), f_const(int), f_load(float *, int);
void f_store(float *, float);
float f_args(float, float, float, float, float, float);

float c_fmul(float a, float b) { return a * b; }

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

static u32 bits(float f) {
  union { float f; u32 u; } v = {f};
  return v.u;
}
static float from_bits(u32 u) {
  union { u32 u; float f; } v = {u};
  return v.f;
}
static void check_f(float got, float want) {
  check((int)bits(got), (int)bits(want));
}

#define QNAN 0x7fc00000u
#define INF 0x7f800000u
#define NINF 0xff800000u

/* No sums, products or quotients of these overflow or become denormal
   (the FPU traps on denormals with FPSW.DN = 0). */
static const float vals[] = {0.0f, -0.0f, 1.0f, -1.0f, 0.5f, 3.25f, -7.125f,
                             1e10f, -3e-10f, 123456.789f, 1e-15f, -2.5e15f};
#define NV (sizeof vals / sizeof vals[0])

int main(void) {
  for (unsigned i = 0; i < NV; i++)
    for (unsigned j = 0; j < NV; j++) {
      float a = vals[i], b = vals[j];
      check_f(fa_add(a, b), a + b);
      check_f(fa_sub(a, b), a - b);
      check_f(fa_mul(a, b), a * b);
      if (b != 0.0f)
        check_f(fa_div(a, b), a / b);
      check(fa_olt(a, b), a < b);
      check(fa_ole(a, b), a <= b);
      check(fa_ogt(a, b), a > b);
      check(fa_oge(a, b), a >= b);
      check(fa_oeq(a, b), a == b);
      check(fa_une(a, b), a != b);
      check_f(fa_max(a, b), a > b ? a : b);
      check(fa_branch(a, b), a < b ? -1 : a == b ? 0 : 1);
      check_f(f_add(a, b), a + b);
      check_f(f_mul(a, b), a * b);
      if (b != 0.0f)
        check_f(f_div(a, b), a / b);
      check(f_olt(a, b), a < b);
      check(f_oeq(a, b), a == b);
      check(f_une(a, b), a != b);
      check(f_uno(a, b), 0);
      check_f(f_sel(a, b), a > b ? a : b);
      check_f(f_copysign(a, b),
              from_bits((bits(a) & 0x7fffffffu) | (bits(b) & 0x80000000u)));
    }
  for (unsigned i = 0; i < NV; i++) {
    float a = vals[i];
    check_f(fa_poly(a), ((a * a) * from_bits(0x3eaaaaab) - 2.5f) * a + 1.0f);
    check_f(f_neg(a), from_bits(bits(a) ^ 0x80000000u));
    check_f(f_abs(a), from_bits(bits(a) & 0x7fffffffu));
    if (a > -2e9f && a < 2e9f)
      check(f_toi(a), (int)a);
    if (a >= 0.0f && a < 4e9f)
      check((int)f_tou(a), (int)(u32)a);
    double d = f_ext(a);
    union { double d; unsigned long long u; } e = {d}, w = {(double)a};
    check((int)e.u, (int)w.u);
    check((int)(e.u >> 32), (int)(w.u >> 32));
    check_f(f_trunc((double)a * 3.0), (float)((double)a * 3.0));
  }

  /* IEEE special values through the soft-float paths */
  float nan = from_bits(QNAN), inf = from_bits(INF), ninf = from_bits(NINF);
  check((int)(bits(f_add(nan, 1.0f)) & 0x7fc00000u), 0x7fc00000);
  check((int)bits(f_add(inf, 1.0f)), INF);
  check((int)(bits(f_add(inf, ninf)) & 0x7f800000u), 0x7f800000);
  check((int)(bits(f_add(inf, ninf)) & 0x007fffffu) != 0, 1); /* NaN */
  check((int)bits(f_div(1.0f, 0.0f)), INF);
  check((int)bits(f_div(-1.0f, 0.0f)), NINF);
  check((int)bits(f_mul(1e30f, 1e30f)), INF);
  check(f_olt(nan, 1.0f), 0);
  check(f_olt(1.0f, nan), 0);
  check(f_ult(nan, 1.0f), 1);
  check(f_ult(2.0f, 1.0f), 0);
  check(f_ult(1.0f, 2.0f), 1);
  check(f_oeq(nan, nan), 0);
  check(f_une(nan, nan), 1);
  check(f_uno(nan, 1.0f), 1);
  check(f_uno(1.0f, nan), 1);
  check(f_olt(ninf, inf), 1);
  check((int)bits(f_sel(nan, 1.0f)), (int)bits(1.0f));
  check((int)bits(f_neg(nan)), (int)(QNAN | 0x80000000u));

  /* conversions, constants, memory, calls */
  static const int ints[] = {0, 1, -1, 7, 16777217, -16777217, 0x7fffffff,
                             (int)0x80000000};
  for (unsigned i = 0; i < 8; i++) {
    check_f(f_fromi(ints[i]), (float)ints[i]);
    check_f(f_fromu((u32)ints[i]), (float)(u32)ints[i]);
  }
  check((int)bits(f_const(0)), (int)bits(1.5f));
  check((int)bits(f_const(1)), (int)0x80000000u);
  float arr[5] = {1.0f, 2.0f, 3.0f, 4.0f, 5.0f};
  check_f(f_load(arr, 2), 3.0f);
  f_store(arr, -9.5f);
  check_f(arr[3], -9.5f);
  check_f(arr[4], 5.0f);
  check_f(f_args(1.5f, 2.0f, 4.0f, 0.25f, 3.0f, 0.5f),
          ((1.5f + 0.25f) - 3.0f * 0.5f) * 2.0f / 4.0f);
  return failures;
}
