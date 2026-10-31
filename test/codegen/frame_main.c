/* Driver for frame.ll: prologues and epilogues (pushm / rtsd). */

int clob_gap(int), clob_one(int), clob_all(int), locals_only(void);
int frame_1020(int), frame_1024(int), frame_big(void);
int stack_args(int, int, int, int, int, int);
long long ret64(long long, int);

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

volatile int seed = 3;

/* a..h are live across each call, so GCC keeps them in R6..R13: a callee
   that does not restore its registers changes the sum. The empty asm makes
   GCC read them from the registers again rather than reuse the sum. */
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

  for (int i = 0; i < 4; i++) {
    check(clob_gap(i), i + 1);
    check(SUM(), want);
    check(clob_one(i), i + 2);
    check(SUM(), want);
    check(clob_all(i), i + 3);
    check(SUM(), want);
    check(locals_only(), elem(5));
    check(frame_1020(251 - i), elem(251 - i));
    check(frame_1020(i), elem(i));
    check(frame_1024(252 - i), elem(252 - i));
    check(frame_big(), elem(255));
    check(SUM(), want);
    check(stack_args(i, 2, 3, 40 + i, 5, 6 + i),
          5 * 100 + (6 + i) * 10000 + i + 40 + i + elem(3));
    check(SUM(), want);
    long long r = ret64(0x123456789LL * (i + 1), i);
    long long w = 0x123456789LL * (i + 1) + elem(i);
    check((int)r, (int)w);
    check((int)(r >> 32), (int)(w >> 32));
    check(SUM(), want);
  }
  return failures != 0;
}
