/* Calls the functions of callee.c (compiled by the other compiler) and
   checks the results against abi.h's references, computed here. The result
   variables are what scripts/qemu-run.sh reads. */

#include "abi.h"

int checks, failures, first_failure = -1;
int got_value, want_value;

static void check(unsigned got, unsigned want) {
  if (got != want && failures++ == 0) {
    first_failure = checks;
    got_value = (int)got;
    want_value = (int)want;
  }
  checks++;
}

static volatile int ks[] = {0, 1, 7, -5, 200, 123456, -98765, 0x7fffffff};
#define N (int)(sizeof(ks) / sizeof(ks[0]))

static unsigned my_cb(unsigned char a, struct S3 b, short c, long long d,
                      struct S8 e, double f, struct S20 g) {
  unsigned h = mix(mix(mix(1, a), hash_S3(b)), c);
  return mix(mix(mix(mix(h, d), hash_S8(e)), dbits(f)), hash_S20(g));
}
static unsigned my_vcb(const char *fmt, ...) {
  va_list ap;
  va_start(ap, fmt);
  unsigned h = v_hash(fmt, ap);
  va_end(ap);
  return h;
}
static unsigned wrap(const char *fmt, ...) {
  va_list ap;
  va_start(ap, fmt);
  unsigned h = v_list(fmt, ap);
  va_end(ap);
  return h;
}

int main(void) {
  for (int i = 0; i < N; i++) {
    int k = ks[i];

    /* Scalars. */
    signed char sc = (signed char)(k * 3);
    unsigned char uc = (unsigned char)(k * 7 + 1);
    short sh = (short)(k * 1001);
    unsigned short us = (unsigned short)(k * 2003 + 5);
    _Bool b = k & 1;
    char ch = (char)(k + 0x80);
    check(h_chars(sc, uc, sh, us, b, ch), ref_chars(sc, uc, sh, us, b, ch));
    check(h_chars_stack(k, k + 1, k + 2, k + 3, sc, uc, sh, us, b, ch, k * 9),
          ref_chars_stack(k, k + 1, k + 2, k + 3, sc, uc, sh, us, b, ch, k * 9));
    check(r_schar(k), ref_schar(k));
    check(r_uchar(k), ref_uchar(k));
    check(r_short(k), ref_short(k));
    check(r_ushort(k), ref_ushort(k));
    check(r_bool(k), k & 1);
    /* Compare the returned char as an int too: the caller relies on the
       callee's extension. */
    check(r_schar(k) + 1000, ref_schar(k) + 1000);
    check(r_ushort(k) * 3, ref_ushort(k) * 3);

    long long ll = ref_ll(k);
    double d = ref_double(k);
    float f = ref_float(k);
    check(h_wide(k, ll, k ^ 5, ~ll, d, f, ll * 3, -d),
          ref_wide(k, ll, k ^ 5, ~ll, d, f, ll * 3, -d));
    check(h_wide2(d, -d, k, d * 2), ref_wide2(d, -d, k, d * 2));
    check(mix(1, r_ll(k)), mix(1, ref_ll(k)));
    check(mix(1, dbits(r_double(k))), mix(1, dbits(ref_double(k))));
    check(fbits(r_float(k)), fbits(ref_float(k)));

    /* Structures. */
#define CHK(T) \
    check(h_a_##T(mk_##T(k), k), mix(hash_##T(mk_##T(k)), k)); \
    check(h_b_##T(k, 2, 3, mk_##T(k), k * 5), \
          mix(mix(mix(mix(hash_##T(mk_##T(k)), k), 2), 3), k * 5)); \
    check(h_c_##T(ll, ~ll, mk_##T(k), k), \
          mix(mix(mix(hash_##T(mk_##T(k)), ll), ~ll), k)); \
    check(hash_##T(r_##T(k)), hash_##T(mk_##T(k)));
    ALL_STRUCTS(CHK)
    check(h_a_U8(mk_U8(k), k), mix(hash_U8(mk_U8(k)), k));
    check(hash_U8(r_U8(k)), hash_U8(mk_U8(k)));

    unsigned r = mix(mix(mix(hash_S3(mk_S3(k)), hash_S1(mk_S1(k))),
                         hash_S4C(mk_S4C(k))), hash_S6(mk_S6(k)));
    r = mix(mix(mix(mix(mix(r, hash_S2(mk_S2(k))), k), hash_S5(mk_S5(k))),
                hash_SP(mk_SP(k))), k * 3);
    check(h_mix(mk_S3(k), mk_S1(k), mk_S4C(k), mk_S6(k), mk_S2(k), k,
                mk_S5(k), mk_SP(k), k * 3), r);
    struct S20 s20 = {{k, 2, 3, 4, k * 5}};
    check(hash_S20(r_S20_args(k, 2, 3, 4, k * 5)), hash_S20(s20));

    /* _Complex. */
    _Complex char cc;
    __real__ cc = k;
    __imag__ cc = k * 3;
    _Complex float cf;
    __real__ cf = f;
    __imag__ cf = -f;
    _Complex double cd;
    __real__ cd = d;
    __imag__ cd = d * 4;
    _Complex short cs;
    __real__ cs = k * 11;
    __imag__ cs = -k;
    unsigned hc = mix(mix(1, __real__ cc), __imag__ cc);
    hc = mix(mix(hc, fbits(__real__ cf)), fbits(__imag__ cf));
    hc = mix(hc, k);
    hc = mix(mix(hc, dbits(__real__ cd)), dbits(__imag__ cd));
    hc = mix(mix(hc, __real__ cc), __imag__ cc);
    hc = mix(mix(hc, __real__ cs), __imag__ cs);
    check(h_cplx(cc, cf, k, cd, cc, cs), hc);
    _Complex char rc = r_cchar(k);
    check(mix(mix(1, __real__ rc), __imag__ rc),
          mix(mix(1, (char)k), (char)-k));
    _Complex float rf = r_cfloat(k);
    check(mix(mix(1, fbits(__real__ rf)), fbits(__imag__ rf)),
          mix(mix(1, 0x40000000u + (unsigned)k), 0xC0000000u - (unsigned)k));
    _Complex double rd = r_cdouble(k);
    check(mix(mix(1, dbits(__real__ rd)), dbits(__imag__ rd)),
          mix(mix(1, 0x4010000000000000ull + (unsigned)k),
              0xC020000000000000ull + (unsigned)k * 7));

    /* Variadic calls. */
    struct S3 s3 = mk_S3(k);
    struct S4C s4 = mk_S4C(k);
    struct S8 s8 = mk_S8(k);
    struct SP sp = mk_SP(k);
    s20 = mk_S20(k);
    void *p = (void *)&ks[i];
    check(v_fmt("i", k), mix(1, k));
    check(v_fmt("3i4", s3, k, s4),
          mix(mix(mix(1, hash_S3(s3)), k), hash_S4C(s4)));
    check(v_fmt("ldpi", ll, d, p, k),
          mix(mix(mix(mix(1, ll), dbits(d)), (unsigned)p), k));
    check(v_fmt("k8s3", s20, s8, sp, s3),
          mix(mix(mix(mix(1, hash_S20(s20)), hash_S8(s8)), hash_SP(sp)),
              hash_S3(s3)));
    check(v_fmt("33s4l", s3, s3, sp, s4, ll),
          mix(mix(mix(mix(mix(1, hash_S3(s3)), hash_S3(s3)), hash_SP(sp)),
                  hash_S4C(s4)), ll));
    unsigned v = mix(mix(mix(mix(1, hash_S3(s3)), ll), hash_S4C(s4)), dbits(d));
    check(v_fmt5(k, 2, 3, 4, "3l4d", s3, ll, s4, d),
          mix(mix(mix(mix(mix(v, k), 2), 3), 4), v));
    unsigned all = mix(mix(mix(mix(mix(mix(mix(mix(1, k), hash_S3(s3)), ll),
                       hash_S4C(s4)), dbits(d)), hash_S20(s20)), hash_S8(s8)),
                       hash_SP(sp));
    check(wrap("i3l4dk8s", k, s3, ll, s4, d, s20, s8, sp), all);

    /* Callbacks: the callee calls this file's functions. */
    check(call_cb(my_cb, k),
          my_cb((unsigned char)(k + 3), mk_S3(k), (short)(k * -5), ref_ll(k),
                mk_S8(k), ref_double(k), mk_S20(k)));
    check(call_vcb(my_vcb, k), all);

    /* Layout: the callee writes the bit-fields as it lays them out. */
    struct BF1 b1;
    struct BF2 b2;
    struct BF3 b3;
    struct BF4 b4;
    struct BF5 b5;
    struct BFP bp;
    fill_BF(&b1, &b2, &b3, &b4, &b5, &bp, k);
    check(hash_BF1(b1), hash_BF1(mk_BF1(k)));
    check(hash_BF2(b2), hash_BF2(mk_BF2(k)));
    check(hash_BF3(b3), hash_BF3(mk_BF3(k)));
    check(hash_BF4(b4), hash_BF4(mk_BF4(k)));
    check(hash_BF5(b5), hash_BF5(mk_BF5(k)));
    check(hash_BFP(bp), hash_BFP(mk_BFP(k)));
  }
  check(sizes(), ref_sizes());
  return failures;
}
