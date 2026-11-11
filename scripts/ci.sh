#!/bin/bash
# The steps of the CI jobs (.github/workflows/ci.yml). They also run by hand
# in an ubuntu:22.04 container, which is what the GitHub runner is.
#
#   ci.sh deps PART...    install the Ubuntu packages PART needs (sudo
#                         unless root)
#   ci.sh build PART      fetch the sources PART needs, build it and pack it
#                         into build/ci/PART.tar.zst
#   ci.sh unpack PART...  unpack build/ci/PART.tar.zst
#   ci.sh sources         fetch the submodules (shallow), for the tests
#
# PART is one of
#   rustc  build/llvm (LLVM with the RX target, lld and the test tools) and
#          the stage1 rustc with stage0's cargo (rust/build)
#   lldb   build/lldb (clang and LLDB, for check-lldb and the LLDB lit tests)
#   gnu    gnu/install (GNU binutils and GCC for rx-elf, for the tests)
#   qemu   ../qemu/build/qemu-system-rx (CDEF-inc/qemu at scripts/qemu.commit)
#   test   what the tests need at run time (deps only)
#   dist   what dist-toolchain.sh needs besides the parts (deps only)
# The packs hold what the tests and dist-toolchain.sh use, not the object
# files.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
host=x86_64-unknown-linux-gnu

deps() {
  local p=()
  for part; do
    case $part in
    rustc) p+=(cmake ninja-build ccache python3 g++ curl git zstd) ;;
    lldb) p+=(cmake ninja-build ccache python3 g++ git curl xz-utils zstd) ;;
    gnu) p+=(g++ make patch bison flex texinfo libgmp-dev libmpfr-dev libmpc-dev wget xz-utils zstd) ;;
    qemu) p+=(git ninja-build python3 python3-venv pkg-config gcc libglib2.0-dev
              libpixman-1-dev libfdt-dev zlib1g-dev zstd) ;;
    test) p+=(git python3 zstd iproute2 libglib2.0-0 libpixman-1-0 libfdt1) ;;
    dist) p+=(patchelf xz-utils binutils) ;;
    *) echo "unknown part $part" >&2; exit 2 ;;
    esac
  done
  local sudo=; [ "$(id -u)" = 0 ] || sudo=sudo
  apt_install() {
    $sudo apt-get update -qq
    $sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
      -o Acquire::Retries=3 --no-install-recommends ca-certificates "${p[@]}" \
      > /dev/null
  }
  apt_install && return
  # The runner's apt mirror list (mirror+file:/etc/apt/apt-mirrors.txt,
  # azure.archive.ubuntu.com first) sometimes stops resolving for hours
  # (2026-11-11): use archive.ubuntu.com instead and try again.
  echo "apt-get install failed: retrying with archive.ubuntu.com" >&2
  local f
  for f in /etc/apt/sources.list /etc/apt/sources.list.d/*; do
    [ -f "$f" ] || continue
    $sudo sed -i -E \
      -e 's#mirror\+file:/etc/apt/apt-mirrors\.txt#http://archive.ubuntu.com/ubuntu/#' \
      -e 's#https?://azure\.archive\.ubuntu\.com/ubuntu/?#http://archive.ubuntu.com/ubuntu/#' \
      "$f"
  done
  apt_install
}

# The submodules, one commit deep (setup.sh).
sources() {
  git -C "$root" config --global --add safe.directory '*' 2>/dev/null || true
  SETUP_DEPTH=1 bash "$root/scripts/setup.sh"
}

# pack PART DIR PATH...: tar PATHs (relative to DIR) into build/ci/PART.tar.zst,
# without the object files and libraries of the build trees build/llvm and
# build/lldb (the tests use their programs and lit configuration)
pack() {
  local part=$1 dir=$2; shift 2
  mkdir -p "$root/build/ci"
  tar -C "$dir" -cf - --exclude='build/ll*/**/*.o' --exclude='build/ll*/**/*.a' \
    --exclude='build/ll*/**/CMakeFiles' "$@" |
    zstd -q -T0 -9 -o "$root/build/ci/$part.tar.zst" -f
  ls -l "$root/build/ci/$part.tar.zst"
}

build() {
  case $1 in
  rustc)
    sources
    bash "$root/scripts/build-llvm.sh"
    bash "$root/scripts/build-llvm-libs.sh"
    bash "$root/scripts/build-rustc.sh"
    pack rustc "$root" build/llvm rust/build/$host/stage1 rust/build/$host/stage0/bin/cargo ;;
  lldb)
    sources
    bash "$root/scripts/build-lldb.sh"
    pack lldb "$root" build/lldb ;;
  gnu)
    bash "$root/scripts/build-gnu.sh"
    pack gnu "$root" gnu/install ;;
  qemu)
    bash "$root/scripts/build-qemu.sh"
    pack qemu "$root/.." qemu/build/qemu-system-rx ;;
  *) echo "unknown part $1" >&2; exit 2 ;;
  esac
}

unpack() {
  local part dir
  for part; do
    dir=$root; [ "$part" = qemu ] && dir=$root/..
    zstd -q -d -c "$root/build/ci/$part.tar.zst" | tar -C "$dir" -xf -
  done
}

cmd=${1:-}; shift || true
case $cmd in
deps) deps "$@" ;;
build) build "$1" ;;
unpack) unpack "$@" ;;
sources) sources ;;
*) sed -n '2,/^set/p' "$0" | sed '$d; s/^# \{0,1\}//' >&2; exit 2 ;;
esac
