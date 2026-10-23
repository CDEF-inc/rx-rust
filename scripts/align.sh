# Prints sizes and alignments of C types as rx-elf-gcc sees them.
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
export PATH=$root/gnu/install/bin:$PATH
d=$(mktemp -d)
cat > $d/a.c <<'EOF'
#define A(t) int a_##t = __alignof__(t); int s_##t = sizeof(t);
typedef long long ll; typedef long double ld; typedef void *ptr;
A(char) A(short) A(int) A(long) A(ll) A(float) A(double) A(ld) A(ptr)
struct S { char c; ll x; }; int off_ll = __builtin_offsetof(struct S, x);
struct D { char c; double x; }; int off_d = __builtin_offsetof(struct D, x);
int big = __BIGGEST_ALIGNMENT__;
int char_signed = (char)-1 < 0;
EOF
rx-elf-gcc -mcpu=rx600 -m64bit-doubles -O2 -S -o - $d/a.c | grep -vE '^\s+\.(global|type|size|section|align|file|ident|balign)'
rm -rf $d
