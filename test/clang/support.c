/* memcpy / memset / memmove for the clang ABI test, which links no C
   library (both compilers may call them to copy structures). Compiled by
   GCC with -ffreestanding -fno-tree-loop-distribute-patterns, so that the
   loops stay loops. */

typedef unsigned long size_t;

void *memcpy(void *d, const void *s, size_t n) {
  unsigned char *p = d;
  const unsigned char *q = s;
  while (n--)
    *p++ = *q++;
  return d;
}

void *memmove(void *d, const void *s, size_t n) {
  unsigned char *p = d;
  const unsigned char *q = s;
  if (p < q) {
    while (n--)
      *p++ = *q++;
  } else {
    while (n--)
      p[n] = q[n];
  }
  return d;
}

void *memset(void *d, int c, size_t n) {
  unsigned char *p = d;
  while (n--)
    *p++ = (unsigned char)c;
  return d;
}
