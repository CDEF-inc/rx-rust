/* The functions that caller.c calls (abi.h), compiled by the other
   compiler. Each one hashes its arguments as it receives them, or returns
   a value made from its argument. */

#include "abi.h"

unsigned h_ext(signed char a, unsigned char b, short c, unsigned short d,
               _Bool e, char f, short g) {
  return ref_ext(a, b, c, d, e, f, g);
}
signed char r_schar(int k) { return ref_schar(k); }
unsigned char r_uchar(int k) { return ref_uchar(k); }
short r_short(int k) { return ref_short(k); }
unsigned short r_ushort(int k) { return ref_ushort(k); }
_Bool r_bool(int k) { return k & 1; }

unsigned h_ex1(unsigned char a, long long b, long long c, short d, int e,
               char f, short g, char h, char i, short j) {
  return ref_ex1(a, b, c, d, e, f, g, h, i, j);
}
unsigned h_fill(int a, int b, int c, long long d, int e) {
  return ref_fill(a, b, c, (u64)d, (unsigned)e, 0);
}
unsigned h_fill2(int a, int b, int c, long long d, int e, long long f) {
  return ref_fill(a, b, c, (u64)d, (unsigned)e, (u64)f);
}
unsigned h_fill_d(int a, int b, int c, double d, float e) {
  return ref_fill(a, b, c, dbits(d), fbits(e), 0);
}
long long r_ll(int k) { return ref_ll(k); }
double r_double(int k) { return ref_double(k); }
float r_float(int k) { return ref_float(k); }

#define DEF(T) \
  unsigned h_a_##T(struct T s, int x) { return mix(hash_##T(s), (unsigned)x); } \
  unsigned h_b_##T(int a, int b, int c, struct T s, int x) { \
    return mix(mix(mix(mix(hash_##T(s), (unsigned)a), (unsigned)b), \
                   (unsigned)c), (unsigned)x); \
  } \
  unsigned h_c_##T(long long a, long long b, struct T s, int x) { \
    return mix(mix(mix(hash_##T(s), (u64)a), (u64)b), (unsigned)x); \
  } \
  struct T r_##T(int k) { return mk_##T(k); }
ALL_STRUCTS(DEF)

unsigned h_stk(struct S6 a, int x, struct S3 b, int y) {
  return mix(mix(mix(hash_S6(a), (unsigned)x), hash_S3(b)), (unsigned)y);
}
struct S20 r_S20_args(int a, int b, int c, int d, int e) {
  struct S20 s;
  s.a[0] = a;
  s.a[1] = b;
  s.a[2] = c;
  s.a[3] = d;
  s.a[4] = e;
  return s;
}

unsigned v_fmt(const char *fmt, ...) {
  va_list ap;
  unsigned h;
  va_start(ap, fmt);
  h = v_hash(fmt, ap);
  va_end(ap);
  return h;
}
unsigned v_fmt4(int a, int b, int c, const char *fmt, ...) {
  va_list ap;
  unsigned h;
  va_start(ap, fmt);
  h = mix(mix(mix(v_hash(fmt, ap), (unsigned)a), (unsigned)b), (unsigned)c);
  va_end(ap);
  return h;
}

unsigned call_cb(cb_t f, int k) {
  return f((unsigned char)(k + 3), mkll((unsigned)k, ~(unsigned)k), mk_S6(k),
           k * 5, (short)(k - 7), mk_S8(k), ref_double(k));
}

unsigned sizes(void) { return ref_sizes(); }
