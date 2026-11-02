# Rewrites llc's RX assembly for GNU as 2.43, which gets two DFPU encodings
# wrong (docs/NOTES-mc.md, NOTES-qemu.md), so that it produces the bytes the
# RXv3 manual / hardware specify:
#   - ftod / itod / utod Rs, DRd: GNU as swaps the two register fields, so
#     it is given the register numbers swapped;
#   - dmov.d with dsp[Rb]: GNU as scales the displacement by 8, the hardware
#     (and LLVM) by 4, so these become .byte.
#
#   awk -f gas-dfpu.awk in.s > out.s
function reg(s) { sub(/^[a-z]+/, "", s); return s + 0 }
function hex(v) { return sprintf("0x%02x", v) }
# dmov.d with a displacement: op8 / op16 the dsp:8 / dsp:16 opcodes.
function dmov(op8, d, dsp, rb,   n, b) {
  n = dsp / 4
  b = "\t.byte\t0xfc, "
  if (n <= 255)
    return b hex(op8) ", " hex(rb * 16 + 8) ", " hex(n) ", " hex(d * 16)
  return b hex(op8 + 1) ", " hex(rb * 16 + 8) ", " hex(n % 256) ", " \
         hex(int(n / 256)) ", " hex(d * 16)
}
/^\t(ftod|itod|utod)\tr[0-9]+, dr[0-9]+$/ {
  split($2, a, ","); printf "\t%s\tr%d, dr%d\n", $1, reg($3), reg(a[1]); next
}
# dmov.d drN, dsp[rB]
/^\tdmov\.d\tdr[0-9]+, [0-9]+\[r[0-9]+\]/ {
  split($3, m, /[\[\]]/); print dmov(0x79, reg($2), m[1] + 0, reg(m[2])); next
}
# dmov.d dsp[rB], drN
/^\tdmov\.d\t[0-9]+\[r[0-9]+\], dr[0-9]+/ {
  split($2, m, /[\[\]]/); print dmov(0xc9, reg($3), m[1] + 0, reg(m[2])); next
}
{ print }
