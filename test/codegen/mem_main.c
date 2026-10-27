/* Driver for mem.ll: memory access forms in code generation. */

void st_imm(void *);
void st_imm_hi(int *, int *, int *, int *, int *, int *, int *, int *);
void st_twice(int *, int *);
int st_stack(int);
int ix_ld_l(int *, int), ix_ld_sw(short *, int), ix_ld_uw(short *, int);
int ix_ld_sb(signed char *, int), ix_ld_ub(signed char *, int);
void ix_st_l(int *, int, int), ix_st_w(short *, int, int);
void ix_st_b(signed char *, int, int);
int ix_glob(int);
int pi_sum_l(int *, int), pi_sum_sw(short *, int);
int pi_sum_ub(unsigned char *, int), pd_sum_sb(signed char *, int);
void pi_fill_w(short *, int), pd_fill_l(int *, int);

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

static union {
  unsigned char b[8192];
  int l[2048];
} buf;

static signed char b8(int off) { return (signed char)buf.b[off]; }
static unsigned char u8(int off) { return buf.b[off]; }
static short w16(int off) { return (short)(buf.b[off] | buf.b[off + 1] << 8); }
static int l32(int off) {
  return buf.b[off] | buf.b[off + 1] << 8 | buf.b[off + 2] << 16 |
         buf.b[off + 3] << 24;
}

static void fill(int v) {
  for (int i = 0; i < 8192; i++)
    buf.b[i] = (unsigned char)v;
}

int main(void) {
  /* st_imm */
  fill(0x55);
  st_imm(buf.b);
  check(b8(0), -1);
  check(u8(1), 0x55); /* only one byte written */
  check(u8(31), 200);
  check(b8(300), -128);
  check(w16(2), 255);
  check(w16(64), -2);
  check(w16(600), 4660);
  check(l32(4), 0);
  check(l32(124), 256);
  check(l32(128), -129);
  check(l32(1020), 8388607);
  check(l32(1024), 305419896);
  check(l32(8000), -8388608);
  check(u8(8004), 0x55);

  /* st_imm_hi */
  fill(0);
  st_imm_hi(&buf.l[0], &buf.l[10], &buf.l[20], &buf.l[30], &buf.l[40],
            &buf.l[50], &buf.l[60], &buf.l[70]);
  for (int i = 0; i < 8; i++)
    check(buf.l[i * 10], i + 1);
  check(buf.l[2], 100);
  check(buf.l[72], 100);

  /* st_twice */
  fill(0);
  st_twice(&buf.l[1], &buf.l[3]);
  check(buf.l[1], 305419896);
  check(buf.l[3], 305419896);
  check(buf.l[2], 0);

  /* st_stack */
  check(st_stack(0), 7);
  check(st_stack(7), -1000);

  /* register-indexed */
  {
    static int l[8] = {1, -2, 3, -4, 0x12345678, 6, 7, 8};
    static short w[8] = {1, -2, 3, -32768, 32767, 6, 7, 8};
    static signed char b[8] = {1, -2, 3, -128, 127, 6, 7, 8};
    check(ix_ld_l(l, 0), 1);
    check(ix_ld_l(l, 4), 0x12345678);
    check(ix_ld_sw(w, 3), -32768);
    check(ix_ld_uw(w, 3), 32768);
    check(ix_ld_sw(w, 4), 32767);
    check(ix_ld_sb(b, 3), -128);
    check(ix_ld_ub(b, 3), 128);
    check(ix_ld_ub(b, 1), 254);
    ix_st_l(l, 7, -99);
    check(l[7], -99);
    check(l[6], 7);
    ix_st_w(w, 5, 0x12345678);
    check(w[5], 0x5678);
    check(w[6], 7);
    ix_st_b(b, 6, 0x1ff);
    check(b[6], -1);
    check(b[7], 8);
    check(b[5], 6);
    for (int i = 0; i < 8; i++)
      check(ix_glob(i), (i + 1) * 10);
  }

  /* post-increment / pre-decrement */
  {
    static int l[5] = {1, -20, 300, -4000, 0x10000};
    static short w[4] = {-1, 32767, -32768, 2};
    static unsigned char ub[4] = {255, 128, 1, 0};
    static signed char sb[4] = {-1, -128, 127, 5};
    check(pi_sum_l(l, 5), 1 - 20 + 300 - 4000 + 0x10000);
    check(pi_sum_l(l, 1), 1);
    check(pi_sum_sw(w, 4), -1 + 32767 - 32768 + 2);
    check(pi_sum_ub(ub, 4), 255 + 128 + 1);
    check(pd_sum_sb(sb + 4, 4), -1 - 128 + 127 + 5);
    check(pd_sum_sb(sb + 4, 1), 5);

    fill(0x55);
    pi_fill_w((short *)buf.b + 1, 3);
    check(w16(0), 0x5555);
    check(w16(2), 0);
    check(w16(4), 1);
    check(w16(6), 2);
    check(w16(8), 0x5555);

    fill(0x55);
    pd_fill_l(&buf.l[5], 3);
    check(buf.l[4], 0);
    check(buf.l[3], 1);
    check(buf.l[2], 2);
    check(buf.l[1], 0x55555555);
    check(buf.l[5], 0x55555555);
  }
  return 0;
}
