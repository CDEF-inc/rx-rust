#!/usr/bin/env python3
"""Check the CFI of rx-rt's interrupt entry in a linked rx-rt application.

    check-isr-cfi.py ELF

For every vector entry from `interrupt!` (`_rx_vector_N`) and for
`_rx_isr_common`, this follows the stack pointer through the disassembly
(rx-elf-objdump) and compares it with the .debug_frame rows
(llvm-dwarfdump) at each instruction:

  - the CFA offset: 8 at a vector entry (the CPU pushed PSW and PC), 36 at
    `_rx_isr_common` (the vector entry pushed R1-R5 and R14-R15), plus
    every push since;
  - at the `jsr` to the handler, the slots of PSW, PC, R1-R5, R14 and R15,
    and the expression for R0 (SP of the interrupted code, from the saved
    USP);
  - `_rx_isr_common_rte` follows `_rx_isr_common` with the CFA at SP + 8.
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GNU = os.path.join(ROOT, "gnu/install/bin")
LLVM = os.path.join(ROOT, "build/llvm/bin")

# Slots relative to the CFA once the vector entry has pushed everything.
SLOTS = {"PSW": -4, "PC": -8, "R5": -12, "R4": -16, "R3": -20, "R2": -24,
         "R1": -28, "R15": -32, "R14": -36}
DCR = ["dpsw", "dcmr", "decnt", "depc"]


def run(*args):
    return subprocess.run(args, check=True, capture_output=True,
                          text=True).stdout


def symbols(elf):
    syms = {}
    for line in run(os.path.join(GNU, "rx-elf-nm"), elf).splitlines():
        f = line.split()
        if len(f) == 3 and f[1] in "tT":
            syms[f[2]] = int(f[0], 16)
    return syms


def cfi_rows(elf):
    """Address -> (CFA offset, {register: CFA-relative slot}, R0 rule)."""
    rows = {}
    out = run(os.path.join(LLVM, "llvm-dwarfdump"), "--debug-frame", elf)
    for m in re.finditer(r"^\s+0x([0-9a-f]+): CFA=R0\+(\d+)(.*)$", out, re.M):
        rest = m.group(3)
        slots = {r: int(o) for r, o in
                 re.findall(r"\b(R\d+|PSW|PC)=\[CFA([+-]\d+)\]", rest)}
        r0 = re.search(r"\bR0=(DW_OP[^=]*?)(?:, R\d+=|, PSW=|, PC=|$)", rest)
        rows[int(m.group(1), 16)] = (int(m.group(2)), slots,
                                     r0.group(1) if r0 else None)
    return rows


def row_at(rows, addr, start):
    best = None
    for a in rows:
        if start <= a <= addr and (best is None or a > best):
            best = a
    return rows[best] if best is not None else None


def disasm(elf, start, stop):
    out = run(os.path.join(GNU, "rx-elf-objdump"), "-d",
              "--start-address=0x%x" % start, "--stop-address=0x%x" % stop,
              elf)
    insns = []
    for line in out.splitlines():
        m = re.match(r"^([0-9a-f]+):\t(?:[0-9a-f]{2} )+\s*\t(\S+)\s*(.*)$",
                     line)
        if m:
            insns.append((int(m.group(1), 16), m.group(2), m.group(3)))
    return insns


def count(ops, step):
    lo, hi = ops.split("-")
    if lo in DCR:
        return step * (DCR.index(hi) - DCR.index(lo) + 1)
    return step * (int(re.sub(r"\D", "", hi)) - int(re.sub(r"\D", "", lo)) + 1)


def sp_change(op, ops):
    """Bytes the instruction pushes (negative: pops)."""
    if op == "pushm":
        return count(ops, 4)
    if op == "popm":
        return -count(ops, 4)
    if op == "push.l":
        return 4
    if op == "pop":
        return -4
    if op == "dpushm.d":
        return count(ops, 8)
    if op == "dpopm.d":
        return -count(ops, 8)
    if op == "dpushm.l":
        return count(ops, 4)
    if op == "dpopm.l":
        return -count(ops, 4)
    m = re.fullmatch(r"#(\d+), r0", ops)
    if op == "add" and m:
        return -int(m.group(1))
    return 0


def check(name, insns, rows, start, cfa, errors):
    saw_jsr = False
    for addr, op, ops in insns:
        row = row_at(rows, addr, start)
        if row is None:
            errors.append("%s: no CFI at 0x%x" % (name, addr))
            return cfa
        if row[0] != cfa:
            errors.append("%s+0x%x (%s %s): CFA offset %d, want %d"
                          % (name, addr - start, op, ops, row[0], cfa))
        if op == "jsr":
            saw_jsr = True
            for reg, slot in SLOTS.items():
                if row[1].get(reg) != slot:
                    errors.append("%s: at the jsr %s is at %s, want CFA%d"
                                  % (name, reg, row[1].get(reg), slot))
            if row[2] is None or "DW_OP_const1u 0x28" not in row[2]:
                errors.append("%s: at the jsr R0 is %s, want the USP "
                              "expression" % (name, row[2]))
        cfa += sp_change(op, ops)
    if name == "_rx_isr_common" and not saw_jsr:
        errors.append("%s: no jsr" % name)
    return cfa


def main():
    elf = sys.argv[1]
    syms = symbols(elf)
    rows = cfi_rows(elf)
    errors = []
    vectors = sorted(a for s, a in syms.items()
                     if re.fullmatch(r"_rx_vector_\d+", s))
    for name in ("_rx_isr_common", "_rx_isr_common_rte"):
        if name not in syms:
            sys.exit("%s: no %s" % (elf, name))
    common, rte = syms["_rx_isr_common"], syms["_rx_isr_common_rte"]
    checked = 0
    for addr in vectors:
        insns = disasm(elf, addr, addr + 16)
        # A vector may be set to another handler (`.set _rx_vector_N, ...`).
        if insns[:1] != [(addr, "pushm", "r1-r5")]:
            continue
        checked += 1
        n = next(i for i, x in enumerate(insns) if x[1] == "bra.a") + 1
        cfa = check("vector@0x%x" % addr, insns[:n], rows, addr, 8, errors)
        if cfa != 36:
            errors.append("vector@0x%x: CFA offset %d at the bra, want 36"
                          % (addr, cfa))
    insns = disasm(elf, common, rte + 2)
    cfa = check("_rx_isr_common", insns, rows, common, 36, errors)
    if cfa != 8:
        errors.append("_rx_isr_common: CFA offset %d at the rte, want 8"
                      % cfa)
    for e in errors:
        print("  " + e)
    print("[isr-cfi] %s: %d vector entries + _rx_isr_common (%d "
          "instructions), %s" % (os.path.basename(elf), checked,
                                 len(insns),
                                 "ok" if not errors else "%d errors"
                                 % len(errors)))
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
