# Sourced by the test scripts: the settings for RX_CPU, a QEMU CPU model of
# CDEF-inc/qemu (rx62n, rxv2, rxv3, rxv3e).
#
#   MCPU            llc's and GNU as's -mcpu
#   QEMU_CPU        qemu-system-rx -cpu (unless already set)
#   RUST_CPU_FLAGS  rustc flags for the core
#
# rx62n is RXv1 with the single-precision FPU (the GCC-compiled test drivers
# use the FPU, so the RXv1 runs use it). Without RX_CPU the scripts keep
# their defaults (RXv1 code, QEMU's default core rx62n).
if [ -n "${RX_CPU:-}" ]; then
  case $RX_CPU in
    rx62n)
      MCPU=rxv1
      RUST_CPU_FLAGS="-C target-cpu=rxv1 -C target-feature=+fpu" ;;
    rxv2|rxv3|rxv3e)
      MCPU=$RX_CPU
      RUST_CPU_FLAGS="-C target-cpu=$RX_CPU" ;;
    *)
      echo "unknown RX_CPU $RX_CPU (rx62n, rxv2, rxv3, rxv3e)" >&2
      exit 2 ;;
  esac
  : "${QEMU_CPU:=$RX_CPU}"
  export MCPU QEMU_CPU RUST_CPU_FLAGS
fi
