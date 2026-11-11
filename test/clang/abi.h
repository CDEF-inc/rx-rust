/* The C ABI test of clang against GCC (scripts/run-clang-test.sh).

   callee.c defines the functions declared here; caller.c calls them and
   checks what they return. The two files are compiled by different
   compilers (clang and rx-elf-gcc, both ways), so every value crosses the
   ABI in both directions: arguments in registers and on the stack, return
   values in registers and in memory, variadic arguments, va_list, callbacks
   and the layout of structures with bit-fields.

   The reference results are the static inline functions here, which the
   caller computes with its own compiler from the same values. */

#ifndef ABI_H
#define ABI_H

#include <stdarg.h>

typedef unsigned long long u64;

/* FNV-style mixing of a value into a hash. */
static inline unsigned mix(unsigned h, u64 v) {
  h = (h ^ (unsigned)v) * 0x01000193u;
  return (h ^ (unsigned)(v >> 32)) * 0x01000193u;
}

/* A double's or float's bits, so that the checks are exact whatever the
   compilers do with floating point. The values that both sides make are
   made from bits too: GCC's -fpu implies -ffinite-math-only, and GNURX
   with -mdfpu folds -(float)0 to +0.0 in one place and -0.0 in another. */
static inline u64 dbits(double d) {
  union { double d; u64 u; } x = {d};
  return x.u;
}
static inline unsigned fbits(float f) {
  union { float f; unsigned u; } x = {f};
  return x.u;
}
static inline double mkd(u64 u) {
  union { u64 u; double d; } x = {u};
  return x.d;
}
static inline float mkf(unsigned u) {
  union { unsigned u; float f; } x = {u};
  return x.f;
}

/* Structures of every size class. */
struct S1 { unsigned char c; };
struct S2 { unsigned char c[2]; };
struct S3 { unsigned char c[3]; };
struct S4 { int a; };
struct S4C { unsigned char c[4]; };
struct SH { short a, b; };
struct S5 { unsigned char c[5]; };
struct S6 { short s[3]; };
struct S7 { unsigned char c[7]; };
struct S8 { int a, b; };
struct SD { double d; };
struct SLL { long long ll; };
struct SF2 { float a, b; };
struct SCD { char c; double d; };
struct S12 { int a, b, c; };
struct S16 { int a[4]; };
struct S20 { int a[5]; };
union U8 { int i[2]; unsigned char c[8]; };
struct __attribute__((aligned(8))) SA8 { int a; };
struct __attribute__((packed)) SP { unsigned char c; int i; };

/* Bit-fields (GCC uses the MS layout on RX, but not for packed ones). */
struct BF1 { int a : 3; char b : 4; short c : 9; };
struct BF2 { char a; int b : 3; unsigned c : 20; };
struct BF3 { char a : 4; char b; int c : 20; long long d : 5; };
struct BF4 { char a; long long b : 40; char c; };
struct BF5 { char a : 3; int : 0; char b; short c : 7; };
struct __attribute__((packed)) BFP { char a : 2; short b : 3; int c : 4; };

/* Hashes of the members and structures made from a seed. */
#define H1(T, x) static inline unsigned hash_##T(struct T s) { unsigned h = 1; x; return h; }
H1(S1, h = mix(h, s.c))
H1(S2, for (int i = 0; i < 2; i++) h = mix(h, s.c[i]))
H1(S3, for (int i = 0; i < 3; i++) h = mix(h, s.c[i]))
H1(S4, h = mix(h, s.a))
H1(S4C, for (int i = 0; i < 4; i++) h = mix(h, s.c[i]))
H1(SH, h = mix(mix(h, s.a), s.b))
H1(S5, for (int i = 0; i < 5; i++) h = mix(h, s.c[i]))
H1(S6, for (int i = 0; i < 3; i++) h = mix(h, s.s[i]))
H1(S7, for (int i = 0; i < 7; i++) h = mix(h, s.c[i]))
H1(S8, h = mix(mix(h, s.a), s.b))
H1(SD, h = mix(h, dbits(s.d)))
H1(SLL, h = mix(h, s.ll))
H1(SF2, h = mix(mix(h, fbits(s.a)), fbits(s.b)))
H1(SCD, h = mix(mix(h, s.c), dbits(s.d)))
H1(S12, h = mix(mix(mix(h, s.a), s.b), s.c))
H1(S16, for (int i = 0; i < 4; i++) h = mix(h, s.a[i]))
H1(S20, for (int i = 0; i < 5; i++) h = mix(h, s.a[i]))
H1(SA8, h = mix(h, s.a))
H1(SP, h = mix(mix(h, s.c), s.i))
H1(BF1, h = mix(mix(mix(h, s.a), s.b), s.c))
H1(BF2, h = mix(mix(mix(h, s.a), s.b), s.c))
H1(BF3, h = mix(mix(mix(mix(h, s.a), s.b), s.c), s.d))
H1(BF4, h = mix(mix(mix(h, s.a), s.b), s.c))
H1(BF5, h = mix(mix(mix(h, s.a), s.b), s.c))
H1(BFP, h = mix(mix(mix(h, s.a), s.b), s.c))
static inline unsigned hash_U8(union U8 u) { return mix(mix(1, u.i[0]), u.i[1]); }

#define MK(T, x) static inline struct T mk_##T(int k) { struct T s; x; return s; }
MK(S1, s.c = k + 1)
MK(S2, for (int i = 0; i < 2; i++) s.c[i] = k * 3 + i)
MK(S3, for (int i = 0; i < 3; i++) s.c[i] = k * 5 + i)
MK(S4, s.a = k * 7)
MK(S4C, for (int i = 0; i < 4; i++) s.c[i] = k ^ (i * 0x41))
MK(SH, s.a = k; s.b = -k * 3)
MK(S5, for (int i = 0; i < 5; i++) s.c[i] = k + i * 11)
MK(S6, for (int i = 0; i < 3; i++) s.s[i] = k * 1000 - i)
MK(S7, for (int i = 0; i < 7; i++) s.c[i] = k * 13 + i)
MK(S8, s.a = k; s.b = ~k)
MK(SD, s.d = mkd(0x4000000000000000ull + (u64)k * 0x123456789ull))
MK(SLL, s.ll = (long long)k * 0x100000001ll - 5)
MK(SF2, s.a = mkf(0x3F000000u + (unsigned)k); s.b = mkf(0xBF800000u ^ (unsigned)k))
MK(SCD, s.c = k; s.d = mkd(0xC010000000000000ull + (unsigned)k))
MK(S12, s.a = k; s.b = k + 100; s.c = k * 7)
MK(S16, for (int i = 0; i < 4; i++) s.a[i] = k * (i + 2))
MK(S20, for (int i = 0; i < 5; i++) s.a[i] = k - i * 9)
MK(SA8, s.a = k * 19)
MK(SP, s.c = k; s.i = k * -77)
MK(BF1, s.a = k; s.b = k >> 1; s.c = k * 3)
MK(BF2, s.a = k; s.b = -k; s.c = k * 4097)
MK(BF3, s.a = k; s.b = k + 1; s.c = k * 999; s.d = -k)
MK(BF4, s.a = k; s.b = (long long)k * 0x10001; s.c = ~k)
MK(BF5, s.a = k; s.b = k * 3; s.c = -k)
MK(BFP, s.a = k; s.b = k + 2; s.c = k * 5)
static inline union U8 mk_U8(int k) { union U8 u; u.i[0] = k; u.i[1] = k * 31; return u; }

/* The functions in callee.c: each returns the reference hash of its
   arguments (h_*), or a value made from them (r_*). */
#define ALL_STRUCTS(X) X(S1) X(S2) X(S3) X(S4) X(S4C) X(SH) X(S5) X(S6) X(S7) \
  X(S8) X(SD) X(SLL) X(SF2) X(SCD) X(S12) X(S16) X(S20) X(SA8) X(SP) X(BF1) \
  X(BF2) X(BF3) X(BF4) X(BF5) X(BFP)

/* Scalars: char and short are not promoted by GCC, neither in registers
   nor on the stack. */
static inline unsigned ref_chars(signed char a, unsigned char b, short c,
                                 unsigned short d, _Bool e, char f) {
  return mix(mix(mix(mix(mix(mix(1, a), b), c), d), e), f);
}
unsigned h_chars(signed char, unsigned char, short, unsigned short, _Bool, char);
unsigned h_chars_stack(int, int, int, int, signed char, unsigned char, short,
                       unsigned short, _Bool, char, int);
static inline unsigned ref_chars_stack(int p, int q, int r, int s,
                                       signed char a, unsigned char b, short c,
                                       unsigned short d, _Bool e, char f, int g) {
  return mix(mix(mix(mix(mix(ref_chars(a, b, c, d, e, f), p), q), r), s), g);
}
signed char r_schar(int);
unsigned char r_uchar(int);
short r_short(int);
unsigned short r_ushort(int);
_Bool r_bool(int);
static inline signed char ref_schar(int k) { return (signed char)(k * 3 - 100); }
static inline unsigned char ref_uchar(int k) { return (unsigned char)(k * 5 + 200); }
static inline short ref_short(int k) { return (short)(k * 777 - 30000); }
static inline unsigned short ref_ushort(int k) { return (unsigned short)(k * 999 + 60000); }

/* 64-bit values in register pairs and on the stack. */
static inline unsigned ref_wide(int a, long long b, int c, long long d,
                                double e, float f, long long g, double h) {
  return mix(mix(mix(mix(mix(mix(mix(mix(1, a), b), c), d), dbits(e)), fbits(f)), g), dbits(h));
}
unsigned h_wide(int, long long, int, long long, double, float, long long, double);
static inline unsigned ref_wide2(double a, double b, int c, double d) {
  return mix(mix(mix(mix(1, dbits(a)), dbits(b)), c), dbits(d));
}
unsigned h_wide2(double, double, int, double);
long long r_ll(int);
double r_double(int);
float r_float(int);
static inline long long ref_ll(int k) { return (long long)k * -0x123456789ll; }
static inline double ref_double(int k) { return mkd(0x3FF0000000000000ull + (unsigned)k * 0x10001ull); }
static inline float ref_float(int k) { return mkf(0x3F800000u + (unsigned)k * 3); }

/* Each structure in three places: first, after three ints (so it may still
   fit in the registers), and after two long longs (on the stack), each time
   followed by an int; and returned. */
#define DECL(T) \
  unsigned h_a_##T(struct T, int); \
  unsigned h_b_##T(int, int, int, struct T, int); \
  unsigned h_c_##T(long long, long long, struct T, int); \
  struct T r_##T(int);
ALL_STRUCTS(DECL)
unsigned h_a_U8(union U8, int);
union U8 r_U8(int);

/* Several small structures packed on the stack. */
unsigned h_mix(struct S3, struct S1, struct S4C, struct S6, struct S2, int,
               struct S5, struct SP, int);
/* A structure returned in memory with arguments filling R1-R4 (the sret
   pointer is in R15). */
struct S20 r_S20_args(int, int, int, int, int);

/* _Complex, which GCC does not treat as an aggregate. */
unsigned h_cplx(_Complex char, _Complex float, int, _Complex double,
                _Complex char, _Complex short);
_Complex char r_cchar(int);
_Complex float r_cfloat(int);
_Complex double r_cdouble(int);

/* Variadic functions: the format says what follows (i int, l long long,
   d double, p pointer, 3 struct S3, 4 struct S4C, 8 struct S8, k struct
   S20, s struct SP). */
unsigned v_fmt(const char *, ...);
unsigned v_fmt5(int, int, int, int, const char *, ...);
unsigned v_list(const char *, va_list);
static inline unsigned v_hash(const char *fmt, va_list ap) {
  unsigned h = 1;
  for (; *fmt; fmt++) {
    switch (*fmt) {
    case 'i': h = mix(h, va_arg(ap, int)); break;
    case 'l': h = mix(h, va_arg(ap, long long)); break;
    case 'd': h = mix(h, dbits(va_arg(ap, double))); break;
    case 'p': h = mix(h, (unsigned)va_arg(ap, void *)); break;
    case '3': h = mix(h, hash_S3(va_arg(ap, struct S3))); break;
    case '4': h = mix(h, hash_S4C(va_arg(ap, struct S4C))); break;
    case '8': h = mix(h, hash_S8(va_arg(ap, struct S8))); break;
    case 'k': h = mix(h, hash_S20(va_arg(ap, struct S20))); break;
    case 's': h = mix(h, hash_SP(va_arg(ap, struct SP))); break;
    }
  }
  return h;
}

/* Callbacks: the callee calls back into the caller's compiler. */
typedef unsigned (*cb_t)(unsigned char, struct S3, short, long long,
                         struct S8, double, struct S20);
unsigned call_cb(cb_t, int);
typedef unsigned (*vcb_t)(const char *, ...);
unsigned call_vcb(vcb_t, int);

/* Layout: the callee fills a structure through a pointer, and returns the
   size of each structure. */
void fill_BF(struct BF1 *, struct BF2 *, struct BF3 *, struct BF4 *,
             struct BF5 *, struct BFP *, int);
unsigned sizes(void);
#define SZ(T) h = mix(h, sizeof(struct T)); h = mix(h, _Alignof(struct T));
static inline unsigned ref_sizes(void) {
  unsigned h = 1;
  ALL_STRUCTS(SZ)
  h = mix(h, _Alignof(long long));
  h = mix(h, _Alignof(double));
  return h;
}

#endif
