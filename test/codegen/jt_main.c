/* Driver for jt.ll: the same switches in C, called for every case value,
   the values around them and the default. */

int L_dense(int), L_offset(int), L_neg(int), L_byte(unsigned char);
int L_two(int, int), L_sparse(int), L_goto(int);

static int hits;
int C_hit(int n) { hits++; return n * 11; }

static int dense(int x) {
  switch (x) {
  case 0: return 17;
  case 1: return -4;
  case 2: return 1000;
  case 3: return 3;
  case 4: return 77;
  case 5: return 0;
  case 6: return 65536;
  case 7: return -99999;
  case 8: return 8;
  case 9: return 123456789;
  default: return -1;
  }
}
static int offset(int x) {
  switch (x) {
  case 100: case 105: return 11;
  case 101: case 106: return 22;
  case 102: case 107: return 33;
  case 104: case 109: return 44;
  case 110: case 111: return 55;
  default: return 0;
  }
}
static int neg(int x) {
  switch (x) {
  case -3: return 30;
  case -2: return 20;
  case -1: return 10;
  case 0: return 5;
  case 1: return 11;
  case 2: return 21;
  case 3: return 31;
  case 4: return 41;
  default: return -7;
  }
}
static int byte(unsigned char x) { return x >= 250 ? x - 249 : 0; }
static int two(int x, int y) {
  int v = x >= 0 && x <= 4 ? (x + 1) * 100 : 0;
  int w = y >= 10 && y <= 14 ? y - 9 : 0;
  return v + w + x;
}
static int sparse(int x) {
  switch (x) {
  case 1: return 1;
  case 1000: return 2;
  case 100000: return 3;
  case -5: return 4;
  default: return 0;
  }
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

static const int far[] = {-2147483647 - 1, -100000, -1000, 1000, 99999,
                          100000, 100001, 2147483647};

int main(void) {
  for (int x = -20; x <= 130; x++) {
    check(L_dense(x), dense(x));
    int h = hits;
    check(L_offset(x), offset(x));
    check(hits - h, offset(x) != 0); /* called once, only for a case */
    check(L_neg(x), neg(x));
    check(L_sparse(x), sparse(x));
    for (int y = 7; y <= 16; y++)
      check(L_two(x, y), two(x, y));
  }
  for (int i = 0; i < (int)(sizeof(far) / sizeof(far[0])); i++) {
    int x = far[i];
    check(L_dense(x), dense(x));
    check(L_offset(x), offset(x));
    check(L_neg(x), neg(x));
    check(L_sparse(x), sparse(x));
    check(L_two(x, x), two(x, x));
  }
  for (int b = 0; b < 256; b++)
    check(L_byte((unsigned char)b), byte((unsigned char)b));
  for (int i = 0; i < 3; i++)
    check(L_goto(i), 7 + i);
  return failures;
}
