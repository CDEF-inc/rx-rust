#!/bin/bash
# Writes a copy of the test/rust/coretests package that builds part of
# library/coretests: some of the test modules (`mod x;` in coretests'
# tests/lib.rs). The whole of coretests does not fit in the RX74N's 1 MB
# code MRAM; `num` (most of the float code) and the rest each do.
# coretests' sources are not changed: the new crate root is tests/lib.rs
# without the other modules' lines, next to symlinks to its module files.
#
#   coretests-part.sh +MOD[,MOD...] DIR   only these modules
#   coretests-part.sh -MOD[,MOD...] DIR   all but these
#
# Build in DIR as in test/rust/coretests (its .cargo/config.toml is copied).
set -eu
spec=$1 dir=$2
root=$(cd "$(dirname "$0")/.." && pwd)
crate=$root/test/rust/coretests
src=$root/rust/library/coretests/tests
case $spec in
  +* | -*) ;;
  *) echo "part: +MOD,... or -MOD,..., not $spec" >&2; exit 2 ;;
esac

mkdir -p "$dir/.cargo"
for f in "$src"/*; do
  [ "$(basename "$f")" = lib.rs ] || ln -sfn "$f" "$dir/$(basename "$f")"
done
awk -v sign="${spec:0:1}" -v list="${spec:1}" '
  BEGIN { n = split(list, m, ","); for (i = 1; i <= n; i++) want[m[i]] = 1 }
  /^mod [a-z_0-9]+;$/ {
    name = substr($2, 1, length($2) - 1)
    if ((sign == "+") != (name in want)) next
  }
  { print }' "$src/lib.rs" > "$dir/lib.rs"
sed -e 's|^path = "../../../rust/library/coretests/tests/lib.rs"|path = "lib.rs"|' \
    -e "s#path = \"\\(std\\|test\\)\"#path = \"$crate/\\1\"#" \
    "$crate/Cargo.toml" > "$dir/Cargo.toml"
grep -q '^path = "lib.rs"' "$dir/Cargo.toml" || { echo "Cargo.toml: no test path" >&2; exit 1; }
cp "$crate/Cargo.lock" "$dir/Cargo.lock"
cp "$crate/.cargo/config.toml" "$dir/.cargo/config.toml"
echo "$dir: $(grep -c '^mod ' "$dir/lib.rs") of $(grep -c '^mod ' "$src/lib.rs") modules"
