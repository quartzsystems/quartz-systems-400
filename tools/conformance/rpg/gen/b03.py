"""Corpus batch 3: expressions and numeric precision. Writes
../b03expr/EXPnnn.rpgle, each a set of EVAL cases with random operand
and result shapes; every case prints its result or the status it ended
with. Seeded, so the members are the same on every run.

usage: python -I b03.py [members] [cases]
"""
import os
import random
import sys

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b03expr")
MEMBERS = int(sys.argv[1]) if len(sys.argv) > 1 else 125
CASES = int(sys.argv[2]) if len(sys.argv) > 2 else 40


def shape(rng, big=False):
    r = rng.random()
    if big and r < 0.15:
        d = rng.randint(32, 63)
    elif r < 0.35:
        d = rng.randint(16, 31)
    else:
        d = rng.randint(1, 15)
    s = rng.randint(0, min(d, 12))
    return d, s


def value(rng, d, s, fill=None):
    """A value that fits d,s, as RPG literal text."""
    i = d - s
    fill = fill if fill is not None else rng.random()
    idig = rng.randint(0, i) if fill < 0.8 else i
    ip = "".join(rng.choice("0123456789") for _ in range(idig)).lstrip("0") or "0"
    fp = "".join(rng.choice("0123456789") for _ in range(s))
    neg = rng.random() < 0.3
    txt = ip + ("." + fp if s else "")
    if neg and txt.strip("0.") != "":
        txt = "-" + txt
    return txt


def packed(d, s):
    return f"packed({d}:{s})" if s else f"packed({d})"


def member_text(n, rng):
    kind = rng.random()
    resdecpos = n % 7 == 3
    lines = ["**FREE"]
    ctl = "ctl-opt dftactgrp(*no)"
    if resdecpos:
        ctl += " expropts(*resdecpos)"
    lines.append(ctl + ";")
    lines.append("dcl-f QSYSPRT printer(132);")
    lines.append("dcl-ds line len(132) end-ds;")
    body = []
    for c in range(1, CASES + 1):
        op = rng.choice("+-*/")
        if n % 10 == 9:
            # Integer operands.
            ta = rng.choice(["int(10)", "int(20)", "int(5)", "uns(10)"])
            tb = rng.choice(["int(10)", "int(20)", "int(3)", "uns(5)"])
            va = str(rng.randint(-99999, 99999)) if "uns" not in ta else str(rng.randint(0, 99999))
            vb = str(rng.randint(-999, 999)) if "uns" not in tb else str(rng.randint(0, 999))
            dr, sr = shape(rng)
            tr = rng.choice([packed(dr, sr), "int(10)", "int(20)"])
        elif n % 10 == 8:
            # Float with decimals.
            ta = "float(8)"
            da, sa = shape(rng)
            tb = packed(da, sa)
            va = value(rng, 9, 4)
            vb = value(rng, da, sa)
            dr, sr = shape(rng)
            tr = packed(dr, sr)
        else:
            da, sa = shape(rng, big=n % 5 == 4)
            db, sb = shape(rng, big=n % 5 == 4)
            dr, sr = shape(rng, big=n % 5 == 4)
            if rng.random() < 0.7:
                # A result field the natural result fits.
                ia, ib = da - sa, db - sb
                need = {"+": max(ia, ib) + 1, "-": max(ia, ib) + 1, "*": ia + ib, "/": ia + sb + 1}[op]
                dr = min(63, max(need + sr, sr + 1, 1))
            ta, tb, tr = packed(da, sa), packed(db, sb), packed(dr, sr)
            if rng.random() < 0.3:
                ta = ta.replace("packed", "zoned")
            va, vb = value(rng, da, sa), value(rng, db, sb)
        if op == "/" and vb.strip("-0.") == "" and rng.random() < 0.7:
            vb = "1"
        half = rng.random() < 0.25
        lines.append(f"dcl-s a{c} {ta} inz({va});")
        lines.append(f"dcl-s b{c} {tb} inz({vb});")
        lines.append(f"dcl-s r{c} {tr};")
        ev = "eval(h) " if half else ""
        body.append("monitor;")
        body.append(f"  {ev}r{c} = a{c} {op} b{c};")
        body.append(f"  line = '{c} ' + %char(r{c});")
        body.append("on-error 102;")
        body.append(f"  line = '{c} E102';")
        body.append("on-error 103;")
        body.append(f"  line = '{c} E103';")
        body.append("on-error;")
        body.append(f"  line = '{c} E';")
        body.append("endmon;")
        body.append("write QSYSPRT line;")
    return "\n".join(lines + body + ["*inlr = *on;"]) + "\n"


os.makedirs(OUT, exist_ok=True)
for n in range(1, MEMBERS + 1):
    rng = random.Random(9000 + n)
    with open(os.path.join(OUT, f"EXP{n:03d}.rpgle"), "w", encoding="utf-8", newline="\n") as f:
        f.write(member_text(n, rng))
print(MEMBERS, "members of", CASES, "cases")
