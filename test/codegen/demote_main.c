/* Driver for demote.ll: results returned by value in the IR that LLVM
   demotes to a hidden sret pointer, against GCC's struct returns. */

struct s6 { unsigned char c[6]; };
struct s20 { int a[5]; };
struct t20 { int a, b, c, d; char e; };
struct q24 { long long a, b, c; };

struct s20 L_d20(int), L_a20(int), L_d5(int, int, int, int, int);
struct s20 L_dv(struct s6, int), L_dsel(int);
struct t20 L_t20(int);
struct q24 L_q24(long long, long long);
int L_cross_d(int);

/* Called from LLVM code. */
struct s20 C_d20(int a) { struct s20 s = {{a, 2, 3, 4, a * 5}}; return s; }
struct q24 C_q24(long long a, long long b) {
  struct q24 s = {a, b, a + b};
  return s;
}
struct s20 C_d5(int a, int b, int c, int d, int e) {
  struct s20 s = {{a, b, c, d, e}};
  return s;
}
static struct s20 C_dsel(int a) {
  struct s20 s = {{0}};
  if (a < 0)
    s.a[4] = -a;
  else
    s.a[0] = a;
  return s;
}

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

static void check_s20(struct s20 got, struct s20 want) {
  for (int i = 0; i < 5; i++)
    check(got.a[i], want.a[i]);
}

static volatile int ks[] = {0, 1, 7, -5, 200, 123456, -98765};
#define N (int)(sizeof(ks) / sizeof(ks[0]))

int main(void) {
  for (int i = 0; i < N; i++) {
    int k = ks[i];
    check_s20(L_d20(k), C_d20(k));
    struct s20 a = L_a20(k);
    struct s20 aw = {{k, -1, k, -1, k}};
    check_s20(a, aw);

    /* The padding after e is not checked: it is undefined. */
    struct t20 t = L_t20(k);
    check(t.a, 1);
    check(t.c, 3);
    check(t.d, k);
    check(t.e, (char)k);

    long long x = (long long)k * 100003, y = -(long long)k << 20;
    struct q24 q = L_q24(x, y);
    check((int)q.a, (int)x);
    check((int)(q.a >> 32), (int)(x >> 32));
    check((int)q.b, (int)y);
    check((int)(q.b >> 32), (int)(y >> 32));
    check((int)q.c, (int)(x + y));
    check((int)(q.c >> 32), (int)((x + y) >> 32));

    check_s20(L_d5(k, k + 1, k + 2, k + 3, k * 9),
              C_d5(k, k + 1, k + 2, k + 3, k * 9));
    struct s6 x6 = {{1, 2, 3, 4, 5, (unsigned char)(k + 9)}};
    struct s20 vw = {{(unsigned char)(k + 9), k, 0, 0, k}};
    check_s20(L_dv(x6, k), vw);
    check_s20(L_dsel(k), C_dsel(k));

    struct s20 c = C_d20(k), d = C_d5(k, 2, 3, 4, 5), m = C_dsel(k);
    int want = c.a[0] + 2 * c.a[4] + 3 * (int)(7 - 3) + 4 * d.a[0] +
               5 * d.a[4] + 6 * k + 7 * m.a[0] + 8 * m.a[4];
    check(L_cross_d(k), want);
  }

  /* GCC returns the sret address in R1; so must the demoted functions. */
  static struct s20 buf;
  int *ret;
  __asm__ volatile("mov.l %1, r15\n\t"
                   "mov.l #6, r1\n\t"
                   "bsr.a _L_d20\n\t"
                   "mov.l r1, %0"
                   : "=r"(ret)
                   : "r"(&buf)
                   : "r1", "r2", "r3", "r4", "r5", "r14", "r15", "memory");
  check(ret == buf.a, 1);
  check(buf.a[4], 30);
  return failures;
}
