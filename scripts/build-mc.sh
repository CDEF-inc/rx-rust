set -e
# Incremental rebuild of llvm-mc and llc after build-llvm.sh has configured
# the tree.
root=$(cd "$(dirname "$0")/.." && pwd)
out=$root/build/llvm
ninja -C "$out" llvm-mc llc > "$out/ninja-mc.log" 2>&1 || {
  grep -E -A12 'error:|FAILED' "$out/ninja-mc.log" | grep -v '^\[' | head -120; exit 1; }
tail -1 "$out/ninja-mc.log"
