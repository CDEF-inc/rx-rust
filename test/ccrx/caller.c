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

static unsigned my_cb(unsigned char a, long long b, struct S6 c, int d,
                      short e, struct S8 f, double g) {
  return mix(mix(mix(mix(mix(mix(mix(1, a), (u64)b), hash_S6(c)), (unsigned)d),
                     (unsigned)(int)e), hash_S8(f)), dbits(g));
}

int main(void) {
  int n;
  for (n = 0; n < N; n++) {
    int k = ks[n];

    /* char / short / _Bool, made so that the upper bits of the registers
       they come from are not their extension. */
    {
      signed char a = (signed char)(k * 3 + 0x180);
      unsigned char b = (unsigned char)(k * 7 + 0x1ff);
      short c = (short)(k * 1001 + 0x18000);
      unsigned short d = (unsigned short)(k * 3001 + 0x1ffff);
      _Bool e = (k & 2) != 0;
      char f = (char)(k + 0x1f0);
      short g = (short)(-k - 0x8000);
      check(h_ext(a, b, c, d, e, f, g), ref_ext(a, b, c, d, e, f, g));
    }
    check((unsigned)(int)r_schar(k), (unsigned)(int)ref_schar(k));
    check((unsigned)(int)r_uchar(k), (unsigned)(int)ref_uchar(k));
    check((unsigned)(int)r_short(k), (unsigned)(int)ref_short(k));
    check((unsigned)(int)r_ushort(k), (unsigned)(int)ref_ushort(k));
    check((unsigned)(int)r_bool(k), (unsigned)(k & 1));

    /* 64-bit pairs, and the arguments after one that went on the stack. */
    {
      long long b = mkll((unsigned)k, 0x89abcdefu ^ (unsigned)k);
      long long c = mkll(~(unsigned)k, (unsigned)k * 3u);
      check(h_ex1((unsigned char)(k + 1), b, c, (short)(k - 4), k * 5,
                  (char)(k + 6), (short)(k * 7), (char)(k + 8), (char)(k + 9),
                  (short)(k - 10)),
            ref_ex1((unsigned char)(k + 1), b, c, (short)(k - 4), k * 5,
                    (char)(k + 6), (short)(k * 7), (char)(k + 8),
                    (char)(k + 9), (short)(k - 10)));
      check(h_fill(k, k + 1, k + 2, b, k * 9),
            ref_fill(k, k + 1, k + 2, (u64)b, (unsigned)(k * 9), 0));
      check(h_fill2(k, k + 1, k + 2, b, k * 9, c),
            ref_fill(k, k + 1, k + 2, (u64)b, (unsigned)(k * 9), (u64)c));
      check(h_fill_d(k, -k, k * 3, ref_double(k), ref_float(k)),
            ref_fill(k, -k, k * 3, dbits(ref_double(k)), fbits(ref_float(k)),
                     0));
    }
    check((unsigned)(u64)r_ll(k), (unsigned)(u64)ref_ll(k));
    check((unsigned)((u64)r_ll(k) >> 32), (unsigned)((u64)ref_ll(k) >> 32));
    check(mix(1, dbits(r_double(k))), mix(1, dbits(ref_double(k))));
    check(fbits(r_float(k)), fbits(ref_float(k)));

    /* Structures as arguments and return values. */
#define CALL(T) \
    { \
      struct T s = mk_##T(k); \
      struct T r; \
      check(h_a_##T(s, k + 11), mix(hash_##T(s), (unsigned)(k + 11))); \
      check(h_b_##T(k, k * 2, k * 3, s, k + 12), \
            mix(mix(mix(mix(hash_##T(s), (unsigned)k), (unsigned)(k * 2)), \
                    (unsigned)(k * 3)), (unsigned)(k + 12))); \
      check(h_c_##T(mkll(1, (unsigned)k), mkll((unsigned)k, 2), s, k + 13), \
            mix(mix(mix(hash_##T(s), (u64)mkll(1, (unsigned)k)), \
                    (u64)mkll((unsigned)k, 2)), (unsigned)(k + 13))); \
      r = r_##T(k); \
      check(hash_##T(r), hash_##T(s)); \
    }
    ALL_STRUCTS(CALL)
    {
      struct S6 a = mk_S6(k);
      struct S3 b = mk_S3(k + 1);
      struct S20 r = r_S20_args(k, k + 1, k + 2, k + 3, k + 4);
      struct S20 w;
      int i;
      check(h_stk(a, k * 3, b, k - 3),
            mix(mix(mix(hash_S6(a), (unsigned)(k * 3)), hash_S3(b)),
                (unsigned)(k - 3)));
      for (i = 0; i < 5; i++)
        w.a[i] = k + i;
      check(hash_S20(r), hash_S20(w));
    }

    /* Variadic arguments. */
    {
      struct S3 s3 = mk_S3(k);
      struct S8 s8 = mk_S8(k);
      struct S20 s20 = mk_S20(k);
      long long l = mkll((unsigned)k, 0x5555u);
      double d = ref_double(k);
      unsigned want;
      want = mix(mix(mix(mix(mix(mix(1, (unsigned)k), (u64)l), dbits(d)),
                         hash_S3(s3)), hash_S8(s8)), hash_S20(s20));
      check(v_fmt("ild38k", k, l, d, s3, s8, s20), want);
      check(v_fmt4(k, k + 1, k + 2, "ild38k", k, l, d, s3, s8, s20),
            mix(mix(mix(want, (unsigned)k), (unsigned)(k + 1)),
                (unsigned)(k + 2)));
    }

    /* A callback into this compiler's code. */
    check(call_cb(my_cb, k),
          my_cb((unsigned char)(k + 3), mkll((unsigned)k, ~(unsigned)k),
                mk_S6(k), k * 5, (short)(k - 7), mk_S8(k), ref_double(k)));
  }
  check(sizes(), ref_sizes());
  return failures;
}
