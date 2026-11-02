/* Driver for fpu_ieee.ll: float and double operations without fast-math
   flags, which use the FPU / DFPU with -mattr=+flush-denormals and libgcc
   without it. The expected results come from libgcc's soft-float functions
   (IEEE 754), called directly: GCC assumes -ffinite-math-only with the FPU,
   so the driver does no float arithmetic or compares of its own. A NaN
   result only has to be a NaN (its sign and payload are not specified);
   cases whose IEEE result is a denormal are skipped (the FPU flushes it). */

typedef unsigned int u32;
typedef unsigned long long u64;

float f_fadd(float, float), f_fsub(float, float), f_fmul(float, float);
float f_fdiv(float, float), f_sel(float, float);
double d_fadd(double, double), d_fsub(double, double), d_fmul(double, double);
double d_fdiv(double, double), d_sel(double, double), d_sqrt(double);
double f_ext(float);
float d_trunc(double);
int f_br(float, float), d_br(double, double);
int f_toi(float), f_sat(float), d_toi(double), d_sat(double);
u32 f_tou(float), f_usat(float), d_tou(double), d_usat(double);
#define PRED(p) int f_##p(float, float), d_##p(double, double);
PRED(oeq) PRED(ogt) PRED(oge) PRED(olt) PRED(ole) PRED(one) PRED(ord)
PRED(ueq) PRED(ugt) PRED(uge) PRED(ult) PRED(ule) PRED(une) PRED(uno)

/* libgcc */
float __addsf3(float, float), __subsf3(float, float), __mulsf3(float, float);
float __divsf3(float, float), __truncdfsf2(double);
double __adddf3(double, double), __subdf3(double, double);
double __muldf3(double, double), __divdf3(double, double);
double __extendsfdf2(float);
int __eqsf2(float, float), __ltsf2(float, float), __lesf2(float, float);
int __gtsf2(float, float), __gesf2(float, float), __unordsf2(float, float);
int __eqdf2(double, double), __ltdf2(double, double), __ledf2(double, double);
int __gtdf2(double, double), __gedf2(double, double);
int __unorddf2(double, double);
int __fixsfsi(float), __fixdfsi(double);
u32 __fixunssfsi(float), __fixunsdfsi(double);

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

static u32 fbits(float f) {
  union { float f; u32 u; } v = {f};
  return v.u;
}
static float fval(u32 u) {
  union { u32 u; float f; } v = {u};
  return v.f;
}
static u64 dbits(double d) {
  union { double d; u64 u; } v = {d};
  return v.u;
}
static double dval(u64 u) {
  union { u64 u; double d; } v = {u};
  return v.d;
}
static int fnan(u32 b) { return (b & 0x7f800000u) == 0x7f800000u && (b & 0x7fffffu); }
static int fdenormal(u32 b) { return (b & 0x7f800000u) == 0 && (b & 0x7fffffu); }
static int dnan(u64 b) {
  return (b & 0x7ff0000000000000ull) == 0x7ff0000000000000ull &&
         (b & 0xfffffffffffffull);
}
static int ddenormal(u64 b) {
  return (b & 0x7ff0000000000000ull) == 0 && (b & 0xfffffffffffffull);
}

/* got is the result of the operation, want libgcc's. */
static void check_f(float got, float want) {
  u32 g = fbits(got), w = fbits(want);
  if (fdenormal(w))
    return;
  if (fnan(w))
    check(fnan(g), 1);
  else
    check((int)g, (int)w);
}
static void check_d(double got, double want) {
  u64 g = dbits(got), w = dbits(want);
  if (ddenormal(w))
    return;
  if (dnan(w)) {
    check(dnan(g), 1);
  } else {
    check((int)g, (int)w);
    check((int)(g >> 32), (int)(w >> 32));
  }
}

/* The 14 predicates from libgcc's compares (each returns the ordered
   answer: false for unordered operands). */
#define PREDS(T, s, x)                                                    \
  static void check_preds_##s(T a, T b) {                                 \
    int un = __unord##x##2(a, b) != 0;                                    \
    int eq = __eq##x##2(a, b) == 0 && !un, lt = __lt##x##2(a, b) < 0;     \
    int le = __le##x##2(a, b) <= 0, gt = __gt##x##2(a, b) > 0;            \
    int ge = __ge##x##2(a, b) >= 0;                                       \
    check(s##_oeq(a, b), eq);                                             \
    check(s##_ogt(a, b), gt);                                             \
    check(s##_oge(a, b), ge);                                             \
    check(s##_olt(a, b), lt);                                             \
    check(s##_ole(a, b), le);                                             \
    check(s##_one(a, b), !un && !eq);                                     \
    check(s##_ord(a, b), !un);                                            \
    check(s##_ueq(a, b), un || eq);                                       \
    check(s##_ugt(a, b), un || gt);                                       \
    check(s##_uge(a, b), un || ge);                                       \
    check(s##_ult(a, b), un || lt);                                       \
    check(s##_ule(a, b), un || le);                                       \
    check(s##_une(a, b), !eq);                                            \
    check(s##_uno(a, b), un);                                             \
    check(s##_br(a, b), un || ge ? 7 : 3);                                \
  }
PREDS(float, f, sf)
PREDS(double, d, df)

static const u32 fv[] = {
  0x00000000, 0x80000000, 0x3f800000, 0xbf800000, 0x3fc00000, 0xc0100000,
  0x7f7fffff, 0xff7fffff, 0x7e967699, 0x1e3ce508, 0x9e3ce508, /* 1e38, 1e-20 */
  0x7f800000, 0xff800000, 0x7fc00000, 0xffc00000, 0x7fc12345, 0x7fffffff,
  0x7fa00000, /* a signalling NaN */
};
#define NF (sizeof fv / sizeof fv[0])
static const u64 dv[] = {
  0x0000000000000000ull, 0x8000000000000000ull, 0x3ff0000000000000ull,
  0xbff0000000000000ull, 0x3ff8000000000000ull, 0xc002000000000000ull,
  0x7fefffffffffffffull, 0xffefffffffffffffull, 0x7e37e43c8800759cull,
  0x1a56e1fc2f8f359dull, 0x9a56e1fc2f8f359dull, /* 1e300, 1e-180 */
  0x7ff0000000000000ull, 0xfff0000000000000ull, 0x7ff8000000000000ull,
  0xfff8000000000000ull, 0x7ff8000012345678ull, 0x7fffffffffffffffull,
  0x7ff4000000000000ull, /* a signalling NaN */
};
#define ND (sizeof dv / sizeof dv[0])

/* Conversions: values around the 32-bit limits. */
static const u32 fc[] = {
  0x00000000, 0x80000000, 0x3f400000, 0xbf400000, 0x42f7cccd, /* .75 123.9 */
  0xc2f7cccd, 0x4effffff, 0x4f000000, 0xcf000000, 0xcf000001, /* 2^31 */
  0x4f7fffff, 0x4f800000, 0xbf800000, 0x7f800000, 0xff800000, /* 2^32 */
  0x7fc00000, 0xffc00000,
};
#define NFC (sizeof fc / sizeof fc[0])
static const u64 dc[] = {
  0x0000000000000000ull, 0x8000000000000000ull, 0x3fe8000000000000ull,
  0xbfe8000000000000ull, 0x41dfffffffc00000ull, 0x41dfffffffe00000ull,
  0x41e0000000000000ull, 0xc1e0000000000000ull, 0xc1e0000000200000ull,
  0x41efffffffe00000ull, 0x41effffffffff000ull, 0x41f0000000000000ull,
  0xbff0000000000000ull, 0x7ff0000000000000ull, 0xfff0000000000000ull,
  0x7ff8000000000000ull, 0xfff8000000000000ull,
};
#define NDC (sizeof dc / sizeof dc[0])

/* x >= 2^31 etc. through libgcc */
static int fge(float a, float b) { return __gesf2(a, b) >= 0; }
static int fle(float a, float b) { return __lesf2(a, b) <= 0; }
static int dge(double a, double b) { return __gedf2(a, b) >= 0; }
static int dle(double a, double b) { return __ledf2(a, b) <= 0; }

/* sqrt for the soft-float d_sqrt (no libm here): exact for the values
   used, with the IEEE special cases. */
double sqrt(double x) {
  u64 b = dbits(x);
  if (dnan(b) || b == 0 || b == 0x8000000000000000ull ||
      b == 0x7ff0000000000000ull)
    return x;
  if (b >> 63)
    return dval(0x7ff8000000000000ull);
  double r = __adddf3(x, 1.0);
  for (int i = 0; i < 80; i++)
    r = __muldf3(0.5, __adddf3(r, __divdf3(x, r)));
  return r;
}

int main(void) {
  for (unsigned i = 0; i < NF; i++)
    for (unsigned j = 0; j < NF; j++) {
      float a = fval(fv[i]), b = fval(fv[j]);
      check_f(f_fadd(a, b), __addsf3(a, b));
      check_f(f_fsub(a, b), __subsf3(a, b));
      check_f(f_fmul(a, b), __mulsf3(a, b));
      check_f(f_fdiv(a, b), __divsf3(a, b));
      check_preds_f(a, b);
      check_f(f_sel(a, b), __gtsf2(a, b) > 0 ? a : b);
    }
  for (unsigned i = 0; i < ND; i++)
    for (unsigned j = 0; j < ND; j++) {
      double a = dval(dv[i]), b = dval(dv[j]);
      check_d(d_fadd(a, b), __adddf3(a, b));
      check_d(d_fsub(a, b), __subdf3(a, b));
      check_d(d_fmul(a, b), __muldf3(a, b));
      check_d(d_fdiv(a, b), __divdf3(a, b));
      check_preds_d(a, b);
      check_d(d_sel(a, b), __gtdf2(a, b) > 0 ? a : b);
    }

  for (unsigned i = 0; i < NFC; i++) {
    float x = fval(fc[i]);
    int nan = fnan(fc[i]);
    /* fptosi / fptoui only where the result is defined */
    if (!nan && fge(x, -2147483648.0f) && __ltsf2(x, 2147483648.0f) < 0)
      check(f_toi(x), __fixsfsi(x));
    if (!nan && __gtsf2(x, -1.0f) > 0 && __ltsf2(x, 4294967296.0f) < 0)
      check((int)f_tou(x), __gesf2(x, 0.0f) >= 0 ? (int)__fixunssfsi(x) : 0);
    /* the saturating ones (Rust's `as`) everywhere */
    int s = nan ? 0
            : fle(x, -2147483648.0f) ? (int)0x80000000
            : fge(x, 2147483648.0f) ? 0x7fffffff
            : __fixsfsi(x);
    check(f_sat(x), s);
    u32 u = nan || fle(x, 0.0f) ? 0
            : fge(x, 4294967296.0f) ? 0xffffffffu
            : __fixunssfsi(x);
    check((int)f_usat(x), (int)u);
  }
  for (unsigned i = 0; i < NDC; i++) {
    double x = dval(dc[i]);
    int nan = dnan(dc[i]);
    if (!nan && __gtdf2(x, -2147483649.0) > 0 && __ltdf2(x, 2147483648.0) < 0)
      check(d_toi(x), __fixdfsi(x));
    if (!nan && __gtdf2(x, -1.0) > 0 && __ltdf2(x, 4294967296.0) < 0)
      check((int)d_tou(x), __gedf2(x, 0.0) >= 0 ? (int)__fixunsdfsi(x) : 0);
    int s = nan ? 0
            : dle(x, -2147483648.0) ? (int)0x80000000
            : dge(x, 2147483647.0) ? 0x7fffffff
            : __fixdfsi(x);
    check(d_sat(x), s);
    u32 u = nan || dle(x, 0.0) ? 0
            : dge(x, 4294967295.0) ? 0xffffffffu
            : __fixunsdfsi(x);
    check((int)d_usat(x), (int)u);
  }

  /* float <-> double */
  for (unsigned i = 0; i < NF; i++)
    check_d(f_ext(fval(fv[i])), __extendsfdf2(fval(fv[i])));
  for (unsigned i = 0; i < ND; i++)
    check_f(d_trunc(dval(dv[i])), __truncdfsf2(dval(dv[i])));
  /* 1 + 2^-24 rounds to even, 1 + 3 * 2^-25 rounds up */
  check_f(d_trunc(dval(0x3ff0000010000000ull)), __truncdfsf2(dval(0x3ff0000010000000ull)));
  check_f(d_trunc(dval(0x3ff0000018000000ull)), __truncdfsf2(dval(0x3ff0000018000000ull)));

  /* sqrt: exact squares and the special cases */
  static const u64 sq[] = {
    0x4010000000000000ull, 0x4002000000000000ull, 0x3fb0000000000000ull,
    0x0000000000000000ull, 0x8000000000000000ull, 0x7ff0000000000000ull,
    0xbff0000000000000ull, 0xfff0000000000000ull, 0x7ff8000000000000ull,
  };
  for (unsigned i = 0; i < sizeof sq / sizeof sq[0]; i++)
    check_d(d_sqrt(dval(sq[i])), sqrt(dval(sq[i])));
  return failures != 0;
}
