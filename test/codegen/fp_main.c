/* Driver for fp.ll: frame pointer and dynamic alloca. */

int fp_locals(int), fp_args(int, int, int, int, int, int), fp_leaf(int);
int fp_big(int), dyn(int), dyn_call(int), dyn_loop(int), dyn_big(int);
int dyn_align(int), dyn_byval(int);

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

void touch(int *p, int n) {
  for (int i = 0; i < n; i++)
    p[i] = i * 7 + 1;
}

static int elem(int i) { return i * 7 + 1; }

int c_sum6(int a, int b, int c, int d, int e, int f) {
  return a + 10 * b + 100 * c + 1000 * d + 10000 * e + 100000 * f;
}

/* Called by dyn_byval with a 3-byte structure on the stack: returns its
   fields and, times 1000, SP mod 4 (read from R0: GCC would fold the
   alignment of a local's address, assuming SP is aligned). */
struct S3 { char a, b, c; };
int c_s3(struct S3 s) {
  unsigned sp;
  __asm__ volatile("mov.l r0, %0" : "=r"(sp));
  return (int)(sp & 3) * 1000 + s.a + 10 * s.b + 100 * s.c;
}

volatile int seed = 3;

/* a..h are live across each call, so GCC keeps them in R6..R13 (see
   frame_main.c). */
#define SUM() \
  ({ \
    __asm__ volatile("" : "+r"(a), "+r"(b), "+r"(c), "+r"(d), "+r"(e), \
                          "+r"(f), "+r"(g), "+r"(h)); \
    a + b + c + d + e + f + g + h; \
  })

int main(void) {
  int a = seed, b = a * 3, c = a + 11, d = b ^ 0x55, e = c * c, f = a - 9;
  int g = b + c, h = d * 5;
  int want = a + b + c + d + e + f + g + h;

  for (int i = 0; i < 6; i++) {
    check(fp_locals(i), elem(i));
    check(SUM(), want);
    check(fp_args(i, 2, 3, 40 + i, 5, 6 + i),
          5 * 100 + (6 + i) * 10000 + i + 40 + i + elem(3));
    check(fp_leaf(i), i + 7);
    check(fp_big(299 - i), elem(299 - i));
    check(SUM(), want);
    check(dyn(i + 1), elem(i));
    check(SUM(), want);
    check(dyn(100 + i), elem(99 + i));
    check(dyn_call(i + 1), c_sum6(1, 2, 3, 4, 5, i + 1) + elem(0));
    check(SUM(), want);
    int acc = 0;
    for (int k = 1; k <= i + 1; k++)
      acc += elem(k - 1);
    check(dyn_loop(i + 1), acc + elem(1));
    check(dyn_big(i + 1), elem(299) + elem(0));
    check(SUM(), want);
    check(dyn_align(i + 2), elem(1));
    check(dyn_byval(i + 1), 321 + elem(0));
    check(SUM(), want);
  }
  /* Many iterations: stackrestore must give the space back. */
  int acc = 0;
  for (int k = 1; k <= 200; k++)
    acc += elem(k - 1);
  check(dyn_loop(200), acc + elem(1));
  return failures != 0;
}
