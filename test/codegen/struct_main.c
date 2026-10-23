/* Driver for struct.ll: structures passed and returned between GCC and
   LLVM code in both directions. */

struct s3 { unsigned char c[3]; };
struct s6 { unsigned char c[6]; };
struct s8 { int a, b; };
struct s12 { int a, b, c; };
struct s16 { int a, b, c, d; };
struct s20 { int a[5]; };
struct sh { short a, b; };

int L_g1(struct s8), L_g2(int, struct s12), L_g3(int, int, struct s12);
int L_g4(struct s3, int), L_g5(struct s6, struct s3, int);
int L_g6(struct s20, int), L_g7(int, int, int, struct s3, int);
int L_g8(struct sh), L_g10(struct s20);
struct s8 L_r8(int);
struct s16 L_r16(int);
struct s20 L_r20(int);
struct s3 L_r3(int);
int L_cross_s(struct s3 *, struct s6 *, struct s20 *, int);

/* Called from LLVM code; same bodies as the L_ versions. */
int C_g3(int a, int b, struct s12 x) { return a * 100 + b + x.a - x.c; }
int C_g4(struct s3 x, int y) { return x.c[2] + y; }
int C_g5(struct s6 x, struct s3 y, int z) { return x.c[5] + y.c[1] * 256 + z; }
int C_g6(struct s20 x, int y) { return x.a[4] - y; }
int C_g7(int a, int b, int c, struct s3 x, int y) {
  return x.c[0] + y * 1000 + a + b + c;
}
struct s8 C_r8(int a) { struct s8 s = {a, a + 1}; return s; }
struct s20 C_r20(int a) { struct s20 s = {{a, 2, 3, 4, a * 5}}; return s; }
struct s3 C_r3(int a) { struct s3 s = {{(unsigned char)a, 2, 3}}; return s; }

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

static volatile int ks[] = {0, 1, 7, -5, 200, 123456, -98765};
#define N (int)(sizeof(ks) / sizeof(ks[0]))

int main(void) {
  for (int i = 0; i < N; i++) {
    int k = ks[i];
    struct s3 x3 = {{(unsigned char)(k + 1), (unsigned char)(k * 3),
                     (unsigned char)(k ^ 0x5a)}};
    struct s6 x6 = {{1, 2, 3, 4, 5, (unsigned char)(k + 9)}};
    struct s8 x8 = {k, k * 2};
    struct s12 x12 = {k, k + 100, k * 7};
    struct s20 x20 = {{k, k + 1, k + 2, k + 3, k * 11}};
    struct sh xh = {(short)k, (short)(k * 3)};

    check(L_g1(x8), x8.b);
    check(L_g2(-k, x12), x12.c + k);
    check(L_g3(3, k, x12), C_g3(3, k, x12));
    check(L_g4(x3, k), C_g4(x3, k));
    check(L_g5(x6, x3, k), C_g5(x6, x3, k));
    check(L_g6(x20, k), C_g6(x20, k));
    check(L_g7(k, 2, 3, x3, -k), C_g7(k, 2, 3, x3, -k));
    check(L_g8(xh), xh.b);
    check(L_g10(x20), 99 + x20.a[1]);
    check(x20.a[0], k); /* the callee's copy was modified, not ours */

    struct s8 r8 = L_r8(k);
    check(r8.a, k);
    check(r8.b, k + 1);
    struct s16 r16 = L_r16(k);
    check(r16.a, k);
    check(r16.d, 4);
    struct s20 r20 = L_r20(k);
    check(r20.a[0], k);
    check(r20.a[3], 4);
    check(r20.a[4], k * 5);
    struct s3 r3 = L_r3(k);
    check(r3.c[0], (unsigned char)k);
    check(r3.c[2], 3);

    struct s12 g3 = {k, k + 1, k + 2};
    struct s8 c8 = C_r8(k);
    struct s20 c20 = C_r20(k);
    struct s3 c3 = C_r3(k);
    int want = C_g4(x3, k) + 2 * C_g5(x6, x3, k) + 3 * C_g6(x20, k) +
               4 * C_g7(1, 2, 3, x3, k) + 5 * C_g3(10, 20, g3) + 6 * c8.a +
               7 * c8.b + 8 * c20.a[4] + 9 * c3.c[2];
    check(L_cross_s(&x3, &x6, &x20, k), want);
  }

  /* GCC returns the sret address in R1; so must LLVM. */
  static struct s20 buf;
  int *ret;
  __asm__ volatile("mov.l %1, r15\n\t"
                   "mov.l #5, r1\n\t"
                   "bsr.a _L_r20\n\t"
                   "mov.l r1, %0"
                   : "=r"(ret)
                   : "r"(&buf)
                   : "r1", "r2", "r3", "r4", "r5", "r14", "r15", "memory");
  check(ret == buf.a, 1);
  check(buf.a[4], 25);
  return failures;
}
