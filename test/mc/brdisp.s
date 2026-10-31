# Branches with a constant displacement (bytes from the start of the
# branch), the form the disassembler prints; checked with scripts/mc-dis.sh
# only (GNU as reads a constant as an address).
bra.s 3
bra.s 7
bra.s 8
bra.s 10
beq.s 3
beq.s 10
bne.s 5
bne.s 9
bra.b -128
bra.b 0
bra.b 127
beq.b -2
bne.b 100
bgeu.b -50
bltu.b 50
bgtu.b 1
bleu.b -1
bpz.b 64
bn.b -64
bge.b 20
blt.b -20
bgt.b 30
ble.b -30
bo.b 4
bno.b -4
bra.w -32768
bra.w 32767
beq.w -300
bne.w 300
bsr.w 1000
bsr.w -1000
bra.a -8388608
bra.a 8388607
bsr.a 65536
bsr.a -65536
