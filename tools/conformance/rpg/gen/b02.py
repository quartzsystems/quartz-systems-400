"""Corpus batch 2: declarations. Scalars of every type with their
lengths, decimals and initial values, in free form and as fixed-form D
specifications; LIKE with adjustments; arrays; named constants; data
structures (qualified, LIKEDS, OVERLAY, POS, DIM, INZ, templates,
BASED). Every member prints what %SIZE, %LEN, %DECPOS, %ELEM and the
value say. Seeded.

usage: python -I b02.py
"""
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fixed import D, wrap  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b02dcl")
FREE_HEAD = ["**FREE", "ctl-opt dftactgrp(*no);", "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"]
M = {}


def p(expr):
    return [f"line = {expr};", "write QSYSPRT line;"]


def shape_of(rng):
    """A scalar type: free-form text, fixed-form (len, type, dec), the
    value an INZ gives, and whether it is numeric."""
    k = rng.choice(["char", "varchar", "packed", "zoned", "bindec", "int", "uns", "float", "ind", "date", "time", "timestamp"])
    if k == "char":
        n = rng.randint(1, 40)
        return f"char({n})", (n, "A", ""), "'" + "abcdefghij"[: min(n, rng.randint(0, 10))] + "'", False
    if k == "varchar":
        n = rng.randint(1, 40)
        return f"varchar({n})", (n, "A", ""), "'" + "xyz"[: min(n, 3)] + "'", False
    if k in ("packed", "zoned"):
        d = rng.randint(1, 31)
        s = rng.randint(0, min(d, 9))
        i = d - s
        txt = (str(rng.randint(0, 10 ** min(i, 9) - 1)) if i else "0") + ("." + "".join(rng.choice("0123456789") for _ in range(s)) if s else "")
        if rng.random() < 0.3:
            txt = "-" + txt
        return f"{k}({d}:{s})", (d, "P" if k == "packed" else "S", s), txt, True
    if k == "bindec":
        d = rng.randint(1, 9)
        s = rng.randint(0, min(d, 4))
        i = d - s
        v = rng.randint(0, 10 ** min(i, 4) - 1) if i else 0
        return f"bindec({d}:{s})", (d, "B", s), str(v), True
    if k in ("int", "uns"):
        d = rng.choice([3, 5, 10, 20])
        v = rng.randint(0, 100)
        return f"{k}({d})", (d, "I" if k == "int" else "U", 0), str(v), True
    if k == "float":
        d = rng.choice([4, 8])
        return f"float({d})", (d, "F", ""), rng.choice(["1.5", "-2.25", "100"]), True
    if k == "ind":
        return "ind", (1, "N", ""), rng.choice(["*on", "*off"]), False
    if k == "date":
        f = rng.choice(["*iso", "*usa", "*eur", "*jis", "*mdy", "*dmy", "*ymd", "*jul"])
        return f"date({f})", ("", "D", ""), "d'2026-03-05'", False
    if k == "time":
        f = rng.choice(["*iso", "*usa", "*eur", "*jis", "*hms"])
        return f"time({f})", ("", "T", ""), "t'14.30.05'", False
    return "timestamp", ("", "Z", ""), "z'2026-03-05-14.30.05.123456'", False


def ff(lines):
    """Free-form lines for a fixed-format member: from column 9, and cut
    to end by column 80 (81 on is the comment area)."""
    return ["        " + piece for l in lines for piece in wrap(l, 72)]


def show(name, numeric, varying=False, decpos=True):
    """The lines that print a field's attributes and value."""
    out = []
    attrs = f"%char(%size({name}))"
    if numeric and decpos:
        attrs += f" + ' ' + %char(%len({name})) + ' ' + %char(%decpos({name}))"
        val = f"%char({name})"
    elif numeric:
        attrs += f" + ' ' + %char(%len({name}))"
        val = f"%char({name})"
    else:
        attrs += f" + ' ' + %char(%len({name}))"
        val = f"'[' + %char({name}) + ']'"
    out += p(f"'{name} ' + {attrs}")
    out += p(f"'  ' + {val}")
    return out


def scalars(name, rng, fixed):
    lines = ["**FREE"] if not fixed else []
    body, decls = [], []
    for i in range(12):
        free, fx, inz, numeric = shape_of(rng)
        fname = f"F{i:02d}"
        use_inz = rng.random() < 0.7
        if fixed:
            ln, typ, dec = fx
            kw = f"INZ({inz})" if use_inz else ""
            if typ == "A" and free.startswith("varchar"):
                kw = (kw + " VARYING").strip()
            if typ in ("D", "T"):
                fmt = free[free.index("(") + 1: -1] if "(" in free else ""
                kw = (kw + (f" DATFMT({fmt})" if typ == "D" else f" TIMFMT({fmt})")).strip()
            if typ == "F":
                ln = ln  # bytes
            decls.append(D(name=fname, deftype="S", to=ln, typ=typ, dec=dec, kw=kw))
        else:
            decls.append(f"dcl-s {fname} {free}" + (f" inz({inz})" if use_inz else "") + ";")
        body += show(fname, numeric, decpos=not free.startswith("float"))
    if fixed:
        lines = [
            "     H DFTACTGRP(*NO)",
            "     FQSYSPRT   O    F  132        PRINTER",
            "     D line            DS           132",
        ] + decls + ["**FREE"] + body + ["*inlr = *on;"]
        # Fixed and free mixed: the free section starts after the specs.
        lines = [l for l in lines if l != "**FREE"]
        lines = lines[:3 + len(decls)] + ["      /free"] + ff(body) + ["        *inlr = *on;", "      /end-free"]
    else:
        lines += FREE_HEAD[1:] + decls + body + ["*inlr = *on;"]
    M[name] = lines


rng = random.Random(902)
for n in range(1, 9):
    scalars(f"DCL{n:03d}", rng, fixed=False)
for n in range(9, 15):
    scalars(f"DCL{n:03d}", rng, fixed=True)

# LIKE with adjustments, named constants, arrays.
M["DCL015"] = FREE_HEAD + [
    "dcl-s base packed(7:2) inz(12.34);",
    "dcl-s more like(base : +3);",
    "dcl-s less like(base : -2);",
    "dcl-s name char(10) inz('abc');",
    "dcl-s longer like(name : +5);",
    "dcl-s vname varchar(8) inz('vv');",
    "dcl-s likev like(vname);",
    "dcl-c K1 'quoted';",
    "dcl-c K2 const(123.45);",
    "dcl-c K3 *blanks;",
    "dcl-c K4 x'C1C2';",
    "more = base * 10;",
    "less = 99.9;",
    "longer = name + 'xyz';",
] + show("more", True) + show("less", True) + show("longer", False) + show("likev", False) + p("K1 + ' ' + %char(K2) + K4") + p("'[' + %char(%len(K1)) + ']'") + p("%char(%len(K1)) + ' ' + %char(%size(K2))") + ["*inlr = *on;"]

M["DCL016"] = FREE_HEAD + [
    "dcl-s a1 packed(5:1) dim(4) inz(2.5);",
    "dcl-s a2 char(3) dim(5);",
    "dcl-s a3 int(10) dim(3) ascend;",
    "dcl-s a4 varchar(6) dim(2) inz('ab');",
    "dcl-s a5 like(a1) dim(7);",
    "dcl-s i int(10);",
    "a2(2) = 'xy';",
    "a3(1) = 3;",
    "a3(2) = 7;",
    "a3(3) = 9;",
    "a5(7) = a1(4) * 3;",
] + p("%char(%elem(a1)) + ' ' + %char(%size(a1)) + ' ' + %char(%size(a1 : *all))") + p("%char(%elem(a2)) + ' ' + %char(%size(a2 : *all))") + p("%char(%elem(a5)) + ' ' + %char(a5(7))") + p("'[' + a2(1) + '][' + a2(2) + ']' + a4(2) + %char(%len(a4(1)))") + [
    "for i = 1 to %elem(a3);",
] + ["  " + l for l in p("%char(i) + '=' + %char(a3(i)) + ' ' + %char(%lookup(a3(i) : a3))")] + ["endfor;", "*inlr = *on;"]

# Data structures.
M["DCL017"] = FREE_HEAD + [
    "dcl-ds d1;",
    "  s1 char(5) inz('abc');",
    "  s2 packed(5:2) inz(1.25);",
    "  s3 zoned(4:0);",
    "  s4 int(5);",
    "end-ds;",
    "dcl-ds d2 qualified inz;",
    "  a char(3);",
    "  b packed(3:0);",
    "  c ind;",
    "end-ds;",
    "dcl-ds d3 len(40);",
    "  p1 char(4) pos(10);",
    "  p2 char(2) pos(30);",
    "end-ds;",
] + p("%char(%size(d1)) + ' ' + %char(%size(d2)) + ' ' + %char(%size(d3))") + p("'[' + s1 + '] ' + %char(s2) + ' ' + %char(s3) + ' ' + %char(s4)") + p("'[' + d2.a + '] ' + %char(d2.b) + ' ' + d2.c") + [
    "p1 = 'WXYZ';",
    "p2 = 'QQ';",
] + p("'[' + d3 + ']'") + ["*inlr = *on;"]

M["DCL018"] = FREE_HEAD + [
    "dcl-ds t template qualified;",
    "  id int(10) inz(7);",
    "  nm char(8) inz('tmpl');",
    "end-ds;",
    "dcl-ds x likeds(t) inz(*likeds);",
    "dcl-ds y likeds(t);",
    "dcl-ds arr likeds(t) dim(3) inz(*likeds);",
    "dcl-ds nest qualified;",
    "  head char(2) inz('hd');",
    "  inner likeds(t);",
    "  tail char(2) inz('tl');",
    "end-ds;",
] + p("%char(x.id) + ' [' + x.nm + '] ' + %char(%size(x))") + p("%char(y.id) + ' [' + y.nm + ']'") + p("%char(%elem(arr)) + ' ' + %char(arr(3).id) + ' ' + %char(%size(arr : *all))") + [
    "nest.inner.id = 55;",
] + p("nest.head + ' ' + %char(nest.inner.id) + ' ' + nest.tail + ' ' + %char(%size(nest))") + ["*inlr = *on;"]

M["DCL019"] = FREE_HEAD + [
    "dcl-ds whole;",
    "  all char(12);",
    "  yy char(4) overlay(all);",
    "  mm char(2) overlay(all : 5);",
    "  dd char(2) overlay(all : *next);",
    "  rest char(4) overlay(all : *next);",
    "end-ds;",
    "dcl-ds nums;",
    "  n1 zoned(4:0) inz(1234);",
    "  n2 zoned(2:0) overlay(n1 : 3);",
    "end-ds;",
    "dcl-ds groups qualified;",
    "  first char(2) inz('ab');",
    "  second char(2) inz('cd');",
    "end-ds;",
    "all = '20260305rest';",
] + p("yy + '/' + mm + '/' + dd + ' ' + rest") + p("%char(n2) + ' ' + %char(%size(nums))") + p("groups.first + groups.second + ' ' + %char(%size(groups))") + ["*inlr = *on;"]

M["DCL020"] = FREE_HEAD + [
    "dcl-s buf char(20) inz('0123456789abcdefghij');",
    "dcl-s p pointer;",
    "dcl-s b1 char(4) based(p);",
    "dcl-ds bds based(p) qualified;",
    "  x char(2);",
    "  y char(3);",
    "end-ds;",
    "p = %addr(buf);",
] + p("b1 + ' ' + bds.x + ' ' + bds.y") + [
    "p = %addr(buf) + 10;",
] + p("b1 + ' ' + bds.x + ' ' + bds.y") + p("%char(%size(p)) + ' ' + %char(%size(bds))") + ["*inlr = *on;"]

# Fixed-form data structures, from-to positions and implicit subfields.
M["DCL021"] = [
    "     H DFTACTGRP(*NO)",
    "     FQSYSPRT   O    F  132        PRINTER",
    "     D line            DS           132",
    "     D ds1             DS",
    "     D  f1                     1      5",
    "     D  f2                     6     10  2",
    "     D  f3                    11     14B 0",
    "     D  f4                    15     22P 3",
    "     D ds2             DS                  INZ",
    "     D  g1                            3",
    "     D  g2                            5S 1",
    "     D  g3                           10I 0",
    "     D ds3             DS                  QUALIFIED",
    "     D  h1                            4    INZ('h1h1')",
    "     D  h2                                 LIKE(h1)",
    "      /free",
    "        f1 = 'abcde';",
    "        f2 = 123.45;",
    "        f3 = -77;",
    "        f4 = 12345.678;",
] + ff(p("f1 + ' ' + %char(f2) + ' ' + %char(f3) + ' ' + %char(f4) + ' ' + %char(%size(ds1))")) + ff(p("'[' + g1 + '] ' + %char(g2) + ' ' + %char(g3) + ' ' + %char(%size(ds2))")) + ff(p("ds3.h1 + '[' + ds3.h2 + '] ' + %char(%size(ds3))")) + [
    "        *inlr = *on;",
    "      /end-free",
]

# Static locals, varying with 4-byte prefix, ASCEND array and DIM by constant.
M["DCL022"] = FREE_HEAD + [
    "dcl-c N 4;",
    "dcl-s arr char(2) dim(N);",
    "dcl-s v4 varchar(10 : 4) inz('four');",
    "dcl-s v2 varchar(10) inz('two');",
    "dcl-s i int(10);",
] + p("%char(%elem(arr)) + ' ' + %char(%size(v4)) + ' ' + %char(%size(v2))") + [
    "for i = 1 to 3;",
] + ["  " + l for l in p("%char(counter())")] + [
    "endfor;",
    "*inlr = *on;",
    "dcl-proc counter;",
    "  dcl-pi *n int(10);",
    "  end-pi;",
    "  dcl-s n int(10) static inz(10);",
    "  dcl-s m int(10) inz(10);",
    "  n += 1;",
    "  m += 1;",
    "  return n * 100 + m;",
    "end-proc;",
]

for name, lines in M.items():
    with open(os.path.join(OUT, name + ".rpgle"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
print(len(M), "members")
