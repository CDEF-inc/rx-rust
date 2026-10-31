#!/usr/bin/env python3
"""Compare the LLVM RX disassembler with GNU objdump on random bytes.

    dis-fuzz.py [-n COUNT] [-s SEED] [--mcpu rxv3]

Each sample (a first byte, then random bytes) goes into its own section of
one object, so both disassemblers start every sample at offset 0. For the
first instruction of each sample this reports:

  - length or mnemonic differences between llvm-objdump and rx-elf-objdump
    (GNU objdump is not ground truth: see docs/NOTES-mc.md);
  - bytes that only one of them decodes;
  - LLVM text that llvm-mc does not assemble back to an instruction with
    the same text (a form may change: the assembler picks the shortest).
"""

import argparse
import collections
import os
import random
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LLVM = os.path.join(ROOT, "build/llvm/bin")
GNU = os.path.join(ROOT, "gnu/install/bin")
GNU_MCPU = {"rxv1": "rx600", "rxv2": "rxv2", "rxv3": "rxv3-dfpu",
            "rxv3e": "rxv3-dfpu"}


def run(args, **kw):
    return subprocess.run(args, capture_output=True, text=True, **kw)


def first_insns(dump, sec_re):
    """{section: (offset of 2nd insn or None, text of 1st insn)}"""
    out = {}
    sec = None
    insns = []
    def flush():
        if sec is not None:
            first = insns[0][1] if insns else None
            nxt = insns[1][0] if len(insns) > 1 else None
            out[sec] = (nxt, first)
    for line in dump.splitlines():
        m = re.match(sec_re, line)
        if m:
            flush()
            sec, insns = int(m.group(1)), []
            continue
        m = re.match(r"\s*([0-9a-f]+):\s+((?:[0-9a-f]{2} )+)\s*(.*)$", line)
        if m and sec is not None:
            text = m.group(3).strip()
            if text:  # GNU continues long byte lists on lines without text
                insns.append((int(m.group(1), 16), text))
    flush()
    return out


# Other names GNU objdump prints for the same instructions.
ALIASES = {"bc": "bgeu", "bnc": "bltu", "bz": "beq", "bnz": "bne",
           "scc": "scgeu", "scnc": "scltu", "scz": "sceq", "scnz": "scne",
           "bmc": "bmgeu", "bmnc": "bmltu", "bmz": "bmeq", "bmnz": "bmne"}


def mnemonic(text):
    if not text:
        return None
    m = text.split()[0].lower()
    base, dot, size = m.partition(".")
    return ALIASES.get(base, base) + dot + size


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("-n", type=int, default=4096)
    ap.add_argument("-s", type=int, default=1)
    ap.add_argument("--mcpu", default="rxv3")
    a = ap.parse_args()
    rnd = random.Random(a.s)
    # Every first byte (and the 06 / fc / fd / fe / ff prefixes more often).
    first = list(range(256)) * (a.n // 256)
    first += [rnd.choice([0x06, 0xfc, 0xfd, 0xfe, 0xff, 0x76, 0x75])
              for _ in range(a.n - len(first))]
    samples = [bytes([b] + [rnd.randrange(256) for _ in range(7)])
               for b in first]

    with tempfile.TemporaryDirectory() as tmp:
        s = os.path.join(tmp, "f.s")
        o = os.path.join(tmp, "f.o")
        with open(s, "w") as f:
            for i, b in enumerate(samples):
                # nops after the sample, so that the second instruction (whose
                # offset is the length of the first) is always there.
                f.write('.section .t%d,"ax"\n.byte %s\n'
                        % (i, ",".join(str(x) for x in b + bytes([3] * 8))))
        r = run([os.path.join(LLVM, "llvm-mc"), "-triple=rx-none-elf",
                 "-mcpu=" + a.mcpu, "-filetype=obj", "-o", o, s])
        if r.returncode:
            sys.exit(r.stderr)
        ld = run([os.path.join(LLVM, "llvm-objdump"), "-d",
                  "--mcpu=" + a.mcpu, o]).stdout
        gd = run([os.path.join(GNU, "rx-elf-objdump"), "-d",
                  "-M", GNU_MCPU[a.mcpu], o]).stdout
        if "unrecognized" in gd[:200] or not gd:
            gd = run([os.path.join(GNU, "rx-elf-objdump"), "-d", o]).stdout

    L = first_insns(ld, r"Disassembly of section \.t(\d+):")
    G = first_insns(gd, r"Disassembly of section \.t(\d+):")

    bad = lambda t: t is None or "unknown" in t or "<unknown>" in t or \
        t.startswith(".") or "invalid" in t
    stats = collections.Counter()
    shown = collections.Counter()
    def show(kind, i, msg):
        stats[kind] += 1
        key = (kind, samples[i][0])
        if shown[key] < 2:
            shown[key] += 1
            print("%-8s %s  %s" % (kind, samples[i][:8].hex(" "), msg))

    roundtrip = []
    for i in range(len(samples)):
        ln, lt = L.get(i, (None, None))
        gn, gt = G.get(i, (None, None))
        lok, gok = not bad(lt), not bad(gt)
        if not lok and not gok:
            stats["both-invalid"] += 1
        elif lok and not gok:
            show("llvm-only", i, "llvm: %s | gnu: %s" % (lt, gt))
        elif gok and not lok:
            show("gnu-only", i, "gnu: %s" % gt)
        else:
            if ln != gn:
                show("length", i, "llvm %s: %s | gnu %s: %s" % (ln, lt, gn, gt))
            elif mnemonic(lt) != mnemonic(gt):
                show("mnemonic", i, "llvm: %s | gnu: %s" % (lt, gt))
            else:
                stats["agree"] += 1
        if lok:
            roundtrip.append((i, lt))

    # Assemble the LLVM text back and disassemble it again: same text.
    with tempfile.TemporaryDirectory() as tmp:
        s = os.path.join(tmp, "r.s")
        o = os.path.join(tmp, "r.o")
        with open(s, "w") as f:
            for i, t in roundtrip:
                f.write('.section .t%d,"ax"\n\t%s\n' % (i, re.sub(r"\s+", " ", t)))
        r = run([os.path.join(LLVM, "llvm-mc"), "-triple=rx-none-elf",
                 "-mcpu=" + a.mcpu, "-filetype=obj", "-o", o, s])
        for line in r.stderr.splitlines():
            if "error:" in line:
                stats["reasm-error"] += 1
                if stats["reasm-error"] <= 20:
                    print("reasm    " + line.split("error:", 1)[1].strip())
        if r.returncode == 0:
            rd = first_insns(run([os.path.join(LLVM, "llvm-objdump"), "-d",
                                  "--mcpu=" + a.mcpu, o]).stdout,
                             r"Disassembly of section \.t(\d+):")
            for i, t in roundtrip:
                t2 = rd.get(i, (None, None))[1]
                norm = lambda x: re.sub(r"\s+", " ", x or "")
                if norm(t) != norm(t2) and "0x" not in t:
                    show("text", i, "%s -> %s" % (t, t2))

    print(dict(stats))


if __name__ == "__main__":
    main()
