/* The C ABI test of clang against CC-RX (scripts/run-ccrx-test.sh).

   callee.c defines the functions declared here; caller.c calls them and
   checks what they return. The two files are compiled by clang and by CC-RX
   (-dbl_size=8, as clang's double is 64 bits) and linked all four ways, so
   every value crosses the calling convention of the CC-RX manual 9.1 in
   both directions. Each group of functions is one pattern of the manual's
   rules: char / short / _Bool extension, 64-bit register pairs, arguments
   after one that went on the stack (which still take the free registers),
   structures by size, return values in R1-R4 and through R15, variadic
   arguments and callbacks.

   This is C99 that CC-RX accepts (-lang=c99): no GNU extensions, and no
   operation that would call a CC-RX run-time routine (the test links no
   CC-RX library): no floating-point arithmetic, no 64-bit multiplication
   or division. The reference results are the static inline functions here,
   which the caller computes with its own compiler from the same values. */

#ifndef ABI_H
#define ABI_H

#include <stdarg.h>

typedef unsigned long long u64;

/* FNV-style mixing of a value into a hash. */
static inline unsigned mix(unsigned h, u64 v) {
  h = (h ^ (unsigned)v) * 0x01000193u;
  return (h ^ (unsigned)(v >> 32)) * 0x01000193u;
}

/* A double's or float's bits, so that the checks are exact. */
static inline u64 dbits(double d) {
  union { double d; u64 u; } x;
  x.d = d;
  return x.u;
}
static inline unsigned fbits(float f) {
  union { float f; unsigned u; } x;
  x.f = f;
  return x.u;
}
static inline double mkd(unsigned hi, unsigned lo) {
  union { u64 u; double d; } x;
  x.u = ((u64)hi << 32) | lo;
  return x.d;
}
static inline float mkf(unsigned u) {
  union { unsigned u; float f; } x;
  x.u = u;
  return x.f;
}
/* A long long from two words (no 64-bit multiplication). */
static inline long long mkll(unsigned hi, unsigned lo) {
  return (long long)(((u64)hi << 32) | lo);
}

/* Structures of every size class: in registers when the size is a multiple
   of 4 and at most 16, else always on the stack. */
struct S1 { unsigned char c; };
struct S2 { unsigned char c[2]; };
struct S3 { unsigned char c[3]; };
struct S4 { int a; };
struct SH { short a, b; };
struct S6 { short s[3]; };
struct S7 { unsigned char c[7]; };
struct S8 { int a, b; };
struct SD { double d; };
struct S12 { int a, b, c; };
struct S16 { int a[4]; };
struct S20 { int a[5]; };

#define HASH(T, n, f) \
  static inline unsigned hash_##T(struct T s) { \
    unsigned h = 1; \
    int i; \
    for (i = 0; i < n; i++) \
      h = mix(h, f); \
    return h; \
  }
HASH(S1, 1, s.c)
HASH(S2, 2, s.c[i])
HASH(S3, 3, s.c[i])
HASH(S4, 1, s.a)
HASH(SH, 1, (unsigned)s.a ^ ((unsigned)s.b << 16))
HASH(S6, 3, s.s[i])
HASH(S7, 7, s.c[i])
HASH(S8, 1, ((u64)(unsigned)s.b << 32) | (unsigned)s.a)
HASH(SD, 1, dbits(s.d))
HASH(S12, 3, i == 0 ? s.a : i == 1 ? s.b : s.c)
HASH(S16, 4, s.a[i])
HASH(S20, 5, s.a[i])

#define MK(T, x) \
  static inline struct T mk_##T(int k) { \
    struct T s; \
    int i; \
    for (i = 0; i < (int)(sizeof(s) / sizeof(char)); i++) \
      ((unsigned char *)&s)[i] = 0; \
    x; \
    return s; \
  }
MK(S1, s.c = (unsigned char)(k + 1))
MK(S2, for (i = 0; i < 2; i++) s.c[i] = (unsigned char)(k * 3 + i))
MK(S3, for (i = 0; i < 3; i++) s.c[i] = (unsigned char)(k * 5 + i))
MK(S4, s.a = k * 7)
MK(SH, s.a = (short)k; s.b = (short)(-k * 3))
MK(S6, for (i = 0; i < 3; i++) s.s[i] = (short)(k * 1000 - i))
MK(S7, for (i = 0; i < 7; i++) s.c[i] = (unsigned char)(k * 13 + i))
MK(S8, s.a = k; s.b = ~k)
MK(SD, s.d = mkd(0x40000000u + (unsigned)k, (unsigned)k * 0x12345u))
MK(S12, s.a = k; s.b = k + 100; s.c = k * 7)
MK(S16, for (i = 0; i < 4; i++) s.a[i] = k * (i + 2))
MK(S20, for (i = 0; i < 5; i++) s.a[i] = k - i * 9)

#define ALL_STRUCTS(X) X(S1) X(S2) X(S3) X(S4) X(SH) X(S6) X(S7) X(S8) \
  X(SD) X(S12) X(S16) X(S20)

/* char, short and _Bool: CC-RX's caller extends them in registers, and its
   callee relies on it (the manual's table 9.2); on the stack they take
   their own size. The callee hashes them as ints, so a value that was not
   extended shows. Four go in R1-R4, the rest on the stack. */
static inline unsigned ref_ext(signed char a, unsigned char b, short c,
                        unsigned short d, _Bool e, char f, short g) {
  return mix(mix(mix(mix(mix(mix(mix(1, (unsigned)(int)a), (unsigned)(int)b),
                                 (unsigned)(int)c), (unsigned)(int)d),
                     (unsigned)(int)e), (unsigned)(int)f), (unsigned)(int)g);
}
unsigned h_ext(signed char, unsigned char, short, unsigned short, _Bool,
               char, short);
/* Return values: extended by the callee. */
signed char r_schar(int);
unsigned char r_uchar(int);
short r_short(int);
unsigned short r_ushort(int);
_Bool r_bool(int);
/* The caller uses them as ints, so a value that was not extended shows. */
static inline signed char ref_schar(int k) { return (signed char)(k * 3 - 100); }
static inline unsigned char ref_uchar(int k) { return (unsigned char)(k * 5 + 200); }
static inline short ref_short(int k) { return (short)(k * 777 - 30000); }
static inline unsigned short ref_ushort(int k) {
  return (unsigned short)(k * 999 + 60000);
}

/* 64-bit values in register pairs (low word in the lower register) and the
   arguments after one that did not fit. */
/* The manual's example 1: R1, R2:R3, stack, R4, then the stack. */
unsigned h_ex1(unsigned char, long long, long long, short, int, char, short,
               char, char, short);
static inline unsigned ref_ex1(unsigned char a, long long b, long long c, short d,
                        int e, char f, short g, char h, char i, short j) {
  return mix(mix(mix(mix(mix(mix(mix(mix(mix(mix(1, a), (u64)b), (u64)c),
                                         (unsigned)(int)d), (unsigned)e),
                             (unsigned)(int)f), (unsigned)(int)g),
                     (unsigned)(int)h), (unsigned)(int)i), (unsigned)(int)j);
}
/* The long long goes on the stack, the last int in R4. */
unsigned h_fill(int, int, int, long long, int);
/* After R4 is taken back, the second long long is on the stack too. */
unsigned h_fill2(int, int, int, long long, int, long long);
/* A double that does not fit, and a float after it in R4. */
unsigned h_fill_d(int, int, int, double, float);
static inline unsigned ref_fill(int a, int b, int c, u64 d, unsigned e, u64 f) {
  return mix(mix(mix(mix(mix(mix(1, (unsigned)a), (unsigned)b), (unsigned)c),
                     d), e), f);
}
long long r_ll(int);
double r_double(int);
float r_float(int);
static inline long long ref_ll(int k) { return mkll(~(unsigned)k, (unsigned)k * 0x10001u); }
static inline double ref_double(int k) {
  return mkd(0x3FF00000u + (unsigned)k, (unsigned)k * 0x10001u);
}
static inline float ref_float(int k) { return mkf(0x3F800000u + (unsigned)k * 3); }

/* Each structure first, after three ints (it may still fit), and after two
   long longs (on the stack), each time followed by an int that takes a
   free register; and returned (R1-R4, or memory through R15). */
#define DECL(T) \
  unsigned h_a_##T(struct T, int); \
  unsigned h_b_##T(int, int, int, struct T, int); \
  unsigned h_c_##T(long long, long long, struct T, int); \
  struct T r_##T(int);
ALL_STRUCTS(DECL)
/* A structure that is always on the stack takes no register: x and y are
   in R1 and R2. */
unsigned h_stk(struct S6, int, struct S3, int);
/* A structure returned in memory while the arguments fill R1-R4. */
struct S20 r_S20_args(int, int, int, int, int);

/* Variadic functions: the unnamed arguments and the last named one are on
   the stack. The format says what follows (i int, l long long, d double,
   3 struct S3, 8 struct S8, k struct S20). */
unsigned v_fmt(const char *, ...);
unsigned v_fmt4(int, int, int, const char *, ...);
static inline unsigned v_hash(const char *fmt, va_list ap) {
  unsigned h = 1;
  for (; *fmt; fmt++) {
    switch (*fmt) {
    case 'i': h = mix(h, (unsigned)va_arg(ap, int)); break;
    case 'l': h = mix(h, (u64)va_arg(ap, long long)); break;
    case 'd': h = mix(h, dbits(va_arg(ap, double))); break;
    case '3': h = mix(h, hash_S3(va_arg(ap, struct S3))); break;
    case '8': h = mix(h, hash_S8(va_arg(ap, struct S8))); break;
    case 'k': h = mix(h, hash_S20(va_arg(ap, struct S20))); break;
    }
  }
  return h;
}

/* Callbacks: the callee calls back into the caller's compiler. */
typedef unsigned (*cb_t)(unsigned char, long long, struct S6, int, short,
                         struct S8, double);
unsigned call_cb(cb_t, int);

/* Layout: the sizes and member offsets of the structures. */
unsigned sizes(void);
#define SZ(T) h = mix(h, sizeof(struct T));
struct OFF { char c; long long ll; short s; double d; char e; };
static inline unsigned ref_sizes(void) {
  unsigned h = 1;
  struct OFF o;
  ALL_STRUCTS(SZ)
  h = mix(h, sizeof(struct OFF));
  h = mix(h, (unsigned)((char *)&o.ll - (char *)&o));
  h = mix(h, (unsigned)((char *)&o.s - (char *)&o));
  h = mix(h, (unsigned)((char *)&o.d - (char *)&o));
  h = mix(h, (unsigned)((char *)&o.e - (char *)&o));
  h = mix(h, sizeof(long double));
  return h;
}

#endif
