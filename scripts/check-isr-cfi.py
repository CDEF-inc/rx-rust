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

A register bank entry (`interrupt!(n, handler, bank = b)`: `save #b` ...
`rstr #b`, then `_rx_vector_N_rte`) has the CFA at SP + 8 and, from the
push of USP to the `add #4, r0`, SP + 12. At its call of
`_rx_isr_bank_call` PSW and PC are in their slots, R1-R5, R14 and R15 are
undefined (they are in the bank) and R0 has the USP expression for
CFA - 12. `_rx_isr_bank_call` starts and ends with the CFA at SP + 4.

It also checks that no DW_AT_producer starts with "clang" or "LLVM": e2
studio's rx-elf-gdb drops the whole .debug_frame of such an ELF.
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
# The same for a register bank entry: the registers in the bank.
BANK_SLOTS = {"PSW": -4, "PC": -8}
BANKED = {"R1", "R2", "R3", "R4", "R5", "R14", "R15"}
CALLS = ("bsr.a", "bsr.w")
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
    """Address -> (CFA offset, {register: CFA-relative slot}, R0 rule,
    {undefined registers})."""
    rows = {}
    out = run(os.path.join(LLVM, "llvm-dwarfdump"), "--debug-frame", elf)
    for m in re.finditer(r"^\s+0x([0-9a-f]+): CFA=R0\+(\d+)(.*)$", out, re.M):
        rest = m.group(3)
        slots = {r: int(o) for r, o in
                 re.findall(r"\b(R\d+|PSW|PC)=\[CFA([+-]\d+)\]", rest)}
        r0 = re.search(r"\bR0=(DW_OP[^=]*?)(?:, R\d+=|, PSW=|, PC=|$)", rest)
        rows[int(m.group(1), 16)] = (int(m.group(2)), slots,
                                     r0.group(1) if r0 else None,
                                     set(re.findall(r"\b(R\d+)=undefined",
                                                    rest)))
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


def check(name, insns, rows, start, cfa, errors, call=None, slots=SLOTS,
          usp=0x28, banked=()):
    """Follows the CFA through INSNS; at CALL (an instruction) checks the
    saved registers and returns the CFA after the last instruction."""
    calls = 0
    for addr, op, ops in insns:
        row = row_at(rows, addr, start)
        if row is None:
            errors.append("%s: no CFI at 0x%x" % (name, addr))
            return cfa
        if row[0] != cfa:
            errors.append("%s+0x%x (%s %s): CFA offset %d, want %d"
                          % (name, addr - start, op, ops, row[0], cfa))
        if call is not None and op in call:
            calls += 1
            for reg, slot in slots.items():
                if row[1].get(reg) != slot:
                    errors.append("%s: at the %s %s is at %s, want CFA%d"
                                  % (name, op, reg, row[1].get(reg), slot))
            if row[2] is None or "DW_OP_const1u 0x%x" % usp not in row[2]:
                errors.append("%s: at the %s R0 is %s, want the USP "
                              "expression" % (name, op, row[2]))
            for reg in sorted(set(banked) - row[3]):
                errors.append("%s: at the %s %s is not undefined"
                              % (name, op, reg))
        cfa += sp_change(op, ops)
    if call is not None and calls != 1:
        errors.append("%s: %d calls of the handler, want 1" % (name, calls))
    return cfa


def check_bank(elf, syms, rows, addr, errors):
    """A register bank vector entry, up to its `rte`."""
    name = next(s for s, a in syms.items()
                if a == addr and re.fullmatch(r"_rx_vector_\d+", s))
    rte = syms.get(name + "_rte")
    if rte is None:
        errors.append("%s: no %s_rte" % (name, name))
        return
    insns = disasm(elf, addr, rte + 2)
    bank = insns[0][2]
    if insns[-1][1] != "rte" or insns[-2][1:] != ("rstr", bank):
        errors.append("%s: does not end with rstr %s; rte" % (name, bank))
    cfa = check(name, insns, rows, addr, 8, errors, call=CALLS,
                slots=BANK_SLOTS, usp=0xc, banked=BANKED)
    if cfa != 8:
        errors.append("%s: CFA offset %d at the rte, want 8" % (name, cfa))


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
    checked = banks = 0
    for addr in vectors:
        insns = disasm(elf, addr, addr + 16)
        if insns[0][1] == "save":
            banks += 1
            check_bank(elf, syms, rows, addr, errors)
            continue
        # A vector may be set to another handler (`.set _rx_vector_N, ...`).
        if insns[:1] != [(addr, "pushm", "r1-r5")]:
            continue
        checked += 1
        # bra.a to _rx_isr_common (bra.w / bra.b after linker relaxation)
        n = next(i for i, x in enumerate(insns)
                 if x[1] in ("bra.a", "bra.w", "bra.b")) + 1
        cfa = check("vector@0x%x" % addr, insns[:n], rows, addr, 8, errors)
        if cfa != 36:
            errors.append("vector@0x%x: CFA offset %d at the bra, want 36"
                          % (addr, cfa))
    insns = disasm(elf, common, rte + 2)
    cfa = check("_rx_isr_common", insns, rows, common, 36, errors,
                call=("jsr",))
    if cfa != 8:
        errors.append("_rx_isr_common: CFA offset %d at the rte, want 8"
                      % cfa)
    if banks:
        call = syms.get("_rx_isr_bank_call")
        if call is None:
            errors.append("no _rx_isr_bank_call")
        else:
            bins = disasm(elf, call, call + 64)
            n = next(i for i, x in enumerate(bins) if x[1] == "rts") + 1
            cfa = check("_rx_isr_bank_call", bins[:n], rows, call, 4, errors)
            if cfa != 4:
                errors.append("_rx_isr_bank_call: CFA offset %d at the rts, "
                              "want 4" % cfa)
    out = run(os.path.join(LLVM, "llvm-dwarfdump"), "--debug-info", elf)
    producers = set(re.findall(r'DW_AT_producer\s+\("([^"]*)"\)', out))
    if not producers:
        errors.append("no DW_AT_producer")
    for p in sorted(producers):
        if p.startswith(("clang", "LLVM")):
            errors.append("producer %r: e2 studio's gdb drops .debug_frame"
                          % p)
    for e in errors:
        print("  " + e)
    print("[isr-cfi] %s: %d vector entries + _rx_isr_common (%d "
          "instructions), %d bank entries, %s"
          % (os.path.basename(elf), checked, len(insns), banks,
                                 "ok" if not errors else "%d errors"
                                 % len(errors)))
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
