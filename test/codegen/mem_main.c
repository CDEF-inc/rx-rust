/* Driver for mem.ll: memory access forms in code generation. */

void st_imm(void *);
void st_imm_hi(int *, int *, int *, int *, int *, int *, int *, int *);
void st_twice(int *, int *);
int st_stack(int);

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
  return 0;
}
