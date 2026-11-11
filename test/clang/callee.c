/* The functions of abi.h, compiled by one compiler and called from
   caller.c compiled by the other. */

#include "abi.h"

unsigned h_chars(signed char a, unsigned char b, short c, unsigned short d,
                 _Bool e, char f) {
  return ref_chars(a, b, c, d, e, f);
}
unsigned h_chars_stack(int p, int q, int r, int s, signed char a,
                       unsigned char b, short c, unsigned short d, _Bool e,
                       char f, int g) {
  return ref_chars_stack(p, q, r, s, a, b, c, d, e, f, g);
}
signed char r_schar(int k) { return ref_schar(k); }
unsigned char r_uchar(int k) { return ref_uchar(k); }
short r_short(int k) { return ref_short(k); }
unsigned short r_ushort(int k) { return ref_ushort(k); }
_Bool r_bool(int k) { return k & 1; }

unsigned h_wide(int a, long long b, int c, long long d, double e, float f,
                long long g, double h) {
  return ref_wide(a, b, c, d, e, f, g, h);
}
unsigned h_wide2(double a, double b, int c, double d) {
  return ref_wide2(a, b, c, d);
}
long long r_ll(int k) { return ref_ll(k); }
double r_double(int k) { return ref_double(k); }
float r_float(int k) { return ref_float(k); }

#define DEF(T) \
  unsigned h_a_##T(struct T x, int y) { return mix(hash_##T(x), y); } \
  unsigned h_b_##T(int a, int b, int c, struct T x, int y) { \
    return mix(mix(mix(mix(hash_##T(x), a), b), c), y); \
  } \
  unsigned h_c_##T(long long a, long long b, struct T x, int y) { \
    return mix(mix(mix(hash_##T(x), a), b), y); \
  } \
  struct T r_##T(int k) { return mk_##T(k); }
ALL_STRUCTS(DEF)
unsigned h_a_U8(union U8 x, int y) { return mix(hash_U8(x), y); }
union U8 r_U8(int k) { return mk_U8(k); }

unsigned h_mix(struct S3 a, struct S1 b, struct S4C c, struct S6 d,
               struct S2 e, int f, struct S5 g, struct SP h, int i) {
  unsigned r = mix(mix(mix(hash_S3(a), hash_S1(b)), hash_S4C(c)), hash_S6(d));
  return mix(mix(mix(mix(mix(r, hash_S2(e)), f), hash_S5(g)), hash_SP(h)), i);
}
struct S20 r_S20_args(int a, int b, int c, int d, int e) {
  struct S20 s = {{a, b, c, d, e}};
  return s;
}

unsigned h_cplx(_Complex char a, _Complex float b, int c, _Complex double d,
                _Complex char e, _Complex short f) {
  unsigned h = mix(mix(1, __real__ a), __imag__ a);
  h = mix(mix(h, fbits(__real__ b)), fbits(__imag__ b));
  h = mix(h, c);
  h = mix(mix(h, dbits(__real__ d)), dbits(__imag__ d));
  h = mix(mix(h, __real__ e), __imag__ e);
  return mix(mix(h, __real__ f), __imag__ f);
}
_Complex char r_cchar(int k) {
  _Complex char c;
  __real__ c = k;
  __imag__ c = -k;
  return c;
}
_Complex float r_cfloat(int k) {
  _Complex float c;
  __real__ c = mkf(0x40000000u + (unsigned)k);
  __imag__ c = mkf(0xC0000000u - (unsigned)k);
  return c;
}
_Complex double r_cdouble(int k) {
  _Complex double c;
  __real__ c = mkd(0x4010000000000000ull + (unsigned)k);
  __imag__ c = mkd(0xC020000000000000ull + (unsigned)k * 7);
  return c;
}

unsigned v_fmt(const char *fmt, ...) {
  va_list ap;
  va_start(ap, fmt);
  unsigned h = v_hash(fmt, ap);
  va_end(ap);
  return h;
}
unsigned v_fmt5(int a, int b, int c, int d, const char *fmt, ...) {
  va_list ap, aq;
  va_start(ap, fmt);
  va_copy(aq, ap);
  unsigned h = mix(mix(mix(mix(v_hash(fmt, ap), a), b), c), d);
  /* Read them again from the copy. */
  h = mix(h, v_hash(fmt, aq));
  va_end(aq);
  va_end(ap);
  return h;
}
unsigned v_list(const char *fmt, va_list ap) { return v_hash(fmt, ap); }

unsigned call_cb(cb_t cb, int k) {
  return cb((unsigned char)(k + 3), mk_S3(k), (short)(k * -5), ref_ll(k),
            mk_S8(k), ref_double(k), mk_S20(k));
}
unsigned call_vcb(vcb_t cb, int k) {
  return cb("i3l4dk8s", k, mk_S3(k), ref_ll(k), mk_S4C(k), ref_double(k),
            mk_S20(k), mk_S8(k), mk_SP(k));
}

void fill_BF(struct BF1 *a, struct BF2 *b, struct BF3 *c, struct BF4 *d,
             struct BF5 *e, struct BFP *f, int k) {
  *a = mk_BF1(k);
  *b = mk_BF2(k);
  *c = mk_BF3(k);
  *d = mk_BF4(k);
  *e = mk_BF5(k);
  *f = mk_BFP(k);
}
unsigned sizes(void) { return ref_sizes(); }
