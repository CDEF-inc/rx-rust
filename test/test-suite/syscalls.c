/* The C start-up and newlib's system calls for llvm-test-suite programs on
   QEMU's RX62N (gdbsim-r5f562n8), run by scripts/test-suite-run.py:
   - the arguments and stdin come from the block the runner loads at
     __args (see struct args); without it argv is {"a.out"} and stdin empty;
   - stdout and stderr go to SCI0, which the runner reads;
   - _exit sends the exit status after a marker and stops;
   - the heap runs from the end of .bss to the argument block
     (rx-qemu-libc.ld).
   GNURX's newlib calls write, read, sbrk, fstat and isatty by these names
   (no underscore), and _exit. Everything else is libnosys's. */

#include <errno.h>
#include <malloc.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/times.h>
#include <time.h>

#define SCI0_TDR (*(volatile unsigned char *)0x00088243)
#define SCI0_SSR (*(volatile unsigned char *)0x00088244)

/* Laid out by test-suite-run.py: argv[argc] is 0 and the stdin bytes
   follow it; the strings come after them. */
struct args {
  unsigned magic; /* ARGS_MAGIC */
  unsigned argc;
  unsigned in_len;
  char *argv[];
};
#define ARGS_MAGIC 0x56475241u /* "ARGV" */
extern struct args __args;

static const char *in;
static unsigned in_left;

static void put(char c) {
  while (!(SCI0_SSR & 0x80)) /* TDRE */
    ;
  SCI0_TDR = c;
}

int write(int fd, const char *buf, int n) {
  (void)fd;
  for (int i = 0; i < n; i++)
    put(buf[i]);
  return n;
}

int read(int fd, char *buf, int n) {
  if (fd != 0)
    return 0;
  if ((unsigned)n > in_left)
    n = in_left;
  memcpy(buf, in, n);
  in += n;
  in_left -= n;
  return n;
}

/* The runner looks for this marker; the status follows in decimal. */
void _exit(int status) {
  static const char marker[] = "\004\004RX-EXIT ";
  char digits[12];
  int n = 0;
  unsigned u = status < 0 ? -(unsigned)status : (unsigned)status;
  for (const char *p = marker; *p; p++)
    put(*p);
  if (status < 0)
    put('-');
  do
    digits[n++] = '0' + u % 10;
  while (u /= 10);
  while (n)
    put(digits[--n]);
  put('\n');
  for (;;)
    __asm__ volatile("wait");
}

extern char _end[];
static char *brk = _end;

void *sbrk(ptrdiff_t incr) {
  char *p = brk;
  if (incr > (char *)&__args - brk) {
    errno = ENOMEM;
    return (void *)-1;
  }
  brk += incr;
  return p;
}

int isatty(int fd) { return fd <= 2; }

int fstat(int fd, struct stat *st) {
  memset(st, 0, sizeof *st);
  st->st_mode = fd <= 2 ? S_IFCHR : 0;
  return 0;
}

/* The clock: CMT0 counts PCLK / 512 (48 MHz / 512 = 93750 Hz) up to
   65535 and interrupts (CMI0, start.s) to count the wraps in
   _rx_clock_high. With QEMU's -icount (test-suite-run.py) its time follows
   the instructions the program ran, so the timings are the same on every
   run. times() (for clock()) and gettimeofday() read it. */
#define PRCR (*(volatile unsigned short *)0x000803fe)
#define MSTPCRA (*(volatile unsigned long *)0x00080010)
#define CMSTR0 (*(volatile unsigned short *)0x00088000)
#define CMCR0 (*(volatile unsigned short *)0x00088002)
#define CMCNT0 (*(volatile unsigned short *)0x00088004)
#define CMCOR0 (*(volatile unsigned short *)0x00088006)
#define IER03 (*(volatile unsigned char *)0x00087203)  /* IEN4: vector 28 */
#define IPR004 (*(volatile unsigned char *)0x00087304) /* CMI0's priority */
#define CMT_HZ 93750u

volatile unsigned long _rx_clock_high;

static void clock_start(void) {
  PRCR = 0xa502; /* MSTPCRA is write-protected */
  MSTPCRA &= ~(1ul << 15); /* CMT unit 0 out of module stop */
  PRCR = 0xa500;
  CMCR0 = 0x0040 | 3; /* CMIE, PCLK / 512 */
  CMCOR0 = 0xffff;
  IPR004 = 1;
  IER03 |= 1 << 4;
  CMSTR0 |= 1;
  __asm__ volatile("setpsw i");
}

/* CMT counts since the start (no wrap is lost: CMI0 raises the high part
   at the wrap, and a wrap between the reads makes them read again). */
static unsigned long long clock_counts(void) {
  unsigned long hi;
  unsigned cnt;
  do {
    hi = _rx_clock_high;
    cnt = CMCNT0;
  } while (hi != _rx_clock_high);
  return ((unsigned long long)hi << 16) | cnt;
}

clock_t times(struct tms *t) {
  clock_t now = clock_counts() * CLOCKS_PER_SEC / CMT_HZ;
  memset(t, 0, sizeof *t);
  t->tms_utime = now;
  return now;
}

int gettimeofday(struct timeval *tv, void *tz) {
  (void)tz;
  unsigned long long c = clock_counts();
  tv->tv_sec = c / CMT_HZ;
  tv->tv_usec = c % CMT_HZ * 1000000 / CMT_HZ;
  return 0;
}

/* Not in newlib. */
int posix_memalign(void **p, size_t align, size_t size) {
  if (align < sizeof(void *) || (align & (align - 1)))
    return EINVAL;
  *p = memalign(align, size);
  return *p ? 0 : ENOMEM;
}

/* What GCC's crtbegin.o would bring: the handle of __cxa_atexit's
   destructors, and the .eh_frame registration for C++ exceptions (libgcc's
   unwinder has no other way to find it here; the function is only linked
   when something throws). */
void *__dso_handle = &__dso_handle;
extern char __eh_frame_start[];
void __register_frame_info(const void *, void *) __attribute__((weak));

int main(int argc, char **argv);
void __libc_init_array(void);
void __libc_fini_array(void);

/* From start.s: as newlib's crt0, then main. */
void _rx_start(void) {
  static char *argv0[] = {"a.out", 0};
  int argc = 1;
  char **argv = argv0;
  if (__args.magic == ARGS_MAGIC) {
    argc = __args.argc;
    argv = __args.argv;
    in = (const char *)&argv[argc + 1];
    in_left = __args.in_len;
  }
  clock_start();
  static long object[8]; /* libgcc's struct object (6 words) */
  if (__register_frame_info)
    __register_frame_info(__eh_frame_start, object);
  atexit(__libc_fini_array);
  __libc_init_array(); /* .init_array, and _init */
  exit(main(argc, argv));
}

/* newlib's __libc_init_array / __libc_fini_array call these. */
void _init(void) {}
void _fini(void) {}
