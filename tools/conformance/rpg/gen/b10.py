"""Corpus batch 10: procedures, prototypes, service programs, binding,
program calls. Writes ../b10proc/*.rpgle and the members they bring
(*.rpginc).

usage: python -I b10.py
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fixed import C, CX  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b10proc")
HEAD = ["**FREE", "ctl-opt dftactgrp(*no);", "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"]
M = {}
INC = {}


def p(expr):
    return [f"line = {expr};", "write QSYSPRT line;"]


# Parameters: by reference, VALUE, CONST with expressions, returns.
M["PRC001"] = HEAD + [
    "dcl-s n packed(7:2) inz(10);",
    "dcl-s c char(10) inz('abc');",
    "dcl-s v varchar(20);",
    "dcl-s f ind;",
    "dcl-s i int(10);",
    "bump(n);",
] + p("'bump ' + %char(n)") + [
    "v = twice('xy');",
] + p("'twice ' + v + ' ' + %char(%len(v))") + [
    "i = sum3(1 : 2 : n * 2);",
] + p("'sum3 ' + %char(i)") + [
    "f = isbig(n + 1000);",
] + p("'isbig ' + f") + [
    "c = %trim(c) + 'd';",
] + p("'val ' + byval(c) + ' ' + c") + p("'fact ' + %char(fact(10))") + ["*inlr = *on;",
    "dcl-proc bump;",
    "  dcl-pi *n;",
    "    x packed(7:2);",
    "  end-pi;",
    "  x += 1.5;",
    "end-proc;",
    "dcl-proc twice;",
    "  dcl-pi *n varchar(20);",
    "    s varchar(10) const;",
    "  end-pi;",
    "  return s + s;",
    "end-proc;",
    "dcl-proc sum3;",
    "  dcl-pi *n int(10);",
    "    a int(10) value;",
    "    b int(10) value;",
    "    c packed(9:2) const;",
    "  end-pi;",
    "  return a + b + %int(c);",
    "end-proc;",
    "dcl-proc isbig;",
    "  dcl-pi *n ind;",
    "    x packed(9:2) value;",
    "  end-pi;",
    "  return x > 500;",
    "end-proc;",
    "dcl-proc byval;",
    "  dcl-pi *n char(10);",
    "    s char(10) value;",
    "  end-pi;",
    "  s = 'changed';",
    "  return s;",
    "end-proc;",
    "dcl-proc fact;",
    "  dcl-pi *n packed(15:0);",
    "    k int(10) value;",
    "  end-pi;",
    "  if k <= 1;",
    "    return 1;",
    "  endif;",
    "  return k * fact(k - 1);",
    "end-proc;",
]

# *NOPASS, %PARMS, *OMIT, %ADDR.
M["PRC002"] = HEAD + [
    "dcl-s a packed(5:0) inz(1);",
] + p("opt(a)") + p("opt(a : 'two')") + p("opt(a : 'two' : 3)") + p("om(*omit)") + p("om(a)") + [
    "*inlr = *on;",
    "dcl-proc opt;",
    "  dcl-pi *n varchar(40);",
    "    x packed(5:0);",
    "    y char(5) const options(*nopass);",
    "    z int(10) value options(*nopass);",
    "  end-pi;",
    "  dcl-s r varchar(40);",
    "  r = 'parms=' + %char(%parms());",
    "  if %parms() >= 2;",
    "    r += ' y=' + %trim(y);",
    "  endif;",
    "  if %parms() >= 3;",
    "    r += ' z=' + %char(z);",
    "  endif;",
    "  return r;",
    "end-proc;",
    "dcl-proc om;",
    "  dcl-pi *n varchar(20);",
    "    x packed(5:0) options(*omit);",
    "  end-pi;",
    "  if %addr(x) = *null;",
    "    return 'omitted';",
    "  endif;",
    "  return 'given ' + %char(x);",
    "end-proc;",
]

# A service program: the module, its program, a caller bound to it.
INC["PRC003S"] = [
    "**FREE",
    "ctl-opt nomain;",
    "dcl-proc greet export;",
    "  dcl-pi *n varchar(30);",
    "    who varchar(10) const;",
    "  end-pi;",
    "  return 'hello ' + who;",
    "end-proc;",
    "dcl-proc addone export;",
    "  dcl-pi *n int(10);",
    "    n int(10) value;",
    "  end-pi;",
    "  return n + 1;",
    "end-proc;",
]
M["PRC003"] = [
    "**FREE",
    "//; with: PRC003S",
    "//; setup: CRTRPGMOD MODULE({lib}/PRC003S) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC003S)",
    "//; setup: CRTSRVPGM SRVPGM({lib}/PRC003S) MODULE({lib}/PRC003S) EXPORT(*ALL)",
    "//; setup: CRTRPGMOD MODULE({lib}/PRC003) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC003)",
    "//; cmd: CRTPGM PGM({lib}/PRC003) MODULE({lib}/PRC003) BNDSRVPGM({lib}/PRC003S)",
    "//; cleanup: DLTSRVPGM SRVPGM({lib}/PRC003S)",
    "//; cleanup: DLTMOD MODULE({lib}/PRC003S)",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
    "dcl-pr greet varchar(30);",
    "  who varchar(10) const;",
    "end-pr;",
    "dcl-pr addone int(10);",
    "  n int(10) value;",
    "end-pr;",
] + p("greet('world')") + p("%char(addone(41))") + ["*inlr = *on;"]

# A prototype brought in with /COPY.
INC["PRC004C"] = [
    "      * The prototype of a procedure, for /COPY.",
    "     D square          PR            10I 0",
    "     D                               10I 0 VALUE",
]
M["PRC004"] = [
    "**FREE",
    "//; with: PRC004C",
    "ctl-opt dftactgrp(*no);",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
    "/copy QRPGLESRC,PRC004C",
] + p("%char(square(12))") + [
    "*inlr = *on;",
    "dcl-proc square;",
    "  dcl-pi *n int(10);",
    "    n int(10) value;",
    "  end-pi;",
    "  return n * n;",
    "end-proc;",
]

# Exported and imported data between two modules of one program.
INC["PRC005D"] = [
    "**FREE",
    "ctl-opt nomain;",
    "dcl-s counter int(10) export inz(100);",
    "dcl-proc tick export;",
    "  counter += 5;",
    "end-proc;",
]
M["PRC005"] = [
    "**FREE",
    "//; with: PRC005D",
    "//; setup: CRTRPGMOD MODULE({lib}/PRC005D) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC005D)",
    "//; setup: CRTRPGMOD MODULE({lib}/PRC005) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC005)",
    "//; cmd: CRTPGM PGM({lib}/PRC005) MODULE({lib}/PRC005 {lib}/PRC005D)",
    "//; cleanup: DLTMOD MODULE({lib}/PRC005D)",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
    "dcl-s counter int(10) import;",
    "dcl-pr tick end-pr;",
] + p("'start ' + %char(counter)") + ["tick();", "tick();"] + p("'after ' + %char(counter)") + ["*inlr = *on;"]

# A program calling another program, by prototype and by CALL/PARM.
INC["PRC006P"] = [
    C(f1="*ENTRY", op="PLIST"),
    C(op="PARM", res="AMT", ln=7, dec=2),
    C(op="PARM", res="TXT", ln=10),
    CX("EVAL", "AMT = AMT * 3"),
    CX("EVAL", "TXT = 'from callee'"),
    C(op="RETURN"),
]
M["PRC006"] = [
    "**FREE",
    "//; with: PRC006P",
    "//; setup: CRTBNDRPG PGM({lib}/PRC006P) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC006P) DBGVIEW(*NONE)",
    "//; cleanup: DLTPGM PGM({lib}/PRC006P)",
    "ctl-opt dftactgrp(*no);",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
    "dcl-pr callee extpgm('PRC006P');",
    "  a packed(7:2);",
    "  t char(10);",
    "end-pr;",
    "dcl-s amt packed(7:2) inz(2.5);",
    "dcl-s txt char(10) inz('before');",
    "callee(amt : txt);",
] + p("%char(amt) + ' ' + txt") + ["*inlr = *on;"]

# Procedure pointers.
M["PRC007"] = HEAD + [
    "dcl-pr op int(10) extproc(fp);",
    "  a int(10) value;",
    "  b int(10) value;",
    "end-pr;",
    "dcl-s fp pointer(*proc);",
    "fp = %paddr(plus);",
] + p("%char(op(3 : 4))") + ["fp = %paddr(times);"] + p("%char(op(3 : 4))") + [
    "*inlr = *on;",
    "dcl-proc plus;",
    "  dcl-pi *n int(10);",
    "    a int(10) value;",
    "    b int(10) value;",
    "  end-pi;",
    "  return a + b;",
    "end-proc;",
    "dcl-proc times;",
    "  dcl-pi *n int(10);",
    "    a int(10) value;",
    "    b int(10) value;",
    "  end-pi;",
    "  return a * b;",
    "end-proc;",
]

# A procedure pointer local to a procedure, with its prototype there.
M["PRC008"] = HEAD + p("%char(viaptr(5))") + [
    "*inlr = *on;",
    "dcl-proc viaptr;",
    "  dcl-pi *n int(10);",
    "    n int(10) value;",
    "  end-pi;",
    "  dcl-pr target int(10) extproc(pt);",
    "    x int(10) value;",
    "  end-pr;",
    "  dcl-s pt pointer(*proc) static;",
    "  pt = %paddr(triple);",
    "  return target(n);",
    "end-proc;",
    "dcl-proc triple;",
    "  dcl-pi *n int(10);",
    "    x int(10) value;",
    "  end-pi;",
    "  return x * 3;",
    "end-proc;",
]

# CALL with PARM factor 1 and 2, a named PLIST, and the callee's %PARMS.
INC["PRC009P"] = [
    "      * Prints how many parameters came, then changes them.",
    "     FQSYSPRT   O    F  132        PRINTER",
    "     D line            DS           132",
    "     D n               S              5I 0",
    C(f1="*ENTRY", op="PLIST"),
    C(op="PARM", res="A", ln=5, dec=0),
    C(op="PARM", res="B", ln=8),
    CX("EVAL", "n = %parms"),
    CX("EVAL", "line = 'callee parms ' + %char(n)"),
    C(op="WRITE", f2="QSYSPRT", res="line"),
    CX("EVAL", "A = A + 100"),
    C(op="IF", f2="n > 1"),
    CX("EVAL", "B = 'changed'"),
    C(op="ENDIF"),
    CX("EVAL", "*INLR = *ON"),
    C(op="RETURN"),
]
M["PRC009"] = [
    "//; with: PRC009P",
    "//; setup: CRTBNDRPG PGM({lib}/PRC009P) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC009P) DBGVIEW(*NONE)",
    "//; cleanup: DLTPGM PGM({lib}/PRC009P)",
    "     FQSYSPRT   O    F  132        PRINTER",
    "     D line            DS           132",
    "     D x               S              5P 0 INZ(7)",
    "     D y               S              8    INZ('before')",
    "     D r               S              5P 0",
    "     D s               S              8",
    C(op="CALL", f2="'PRC009P'"),
    C(f1="r", op="PARM", f2="x", res="P1", ln=5, dec=0),
    C(f1="s", op="PARM", f2="y", res="P2", ln=8),
    CX("EVAL", "line = %char(x) + ' ' + y + ' ' + %char(r) + ' ' + s"),
    C(op="WRITE", f2="QSYSPRT", res="line"),
    C(f1="PL", op="PLIST"),
    C(op="PARM", res="x"),
    C(op="CALL", f2="'PRC009P'", res="PL"),
    CX("EVAL", "line = %char(x)"),
    C(op="WRITE", f2="QSYSPRT", res="line"),
    CX("EVAL", "*INLR = *ON"),
]
# Fixed-form members start in column 1 with the directives as comments.
M["PRC009"] = ["      *; " + l[4:] if l.startswith("//; ") else l for l in M["PRC009"]]

# A linear-main program with parameters, called with all and with fewer.
INC["PRC010P"] = [
    "**FREE",
    "ctl-opt main(entry) dftactgrp(*no);",
    "dcl-proc entry;",
    "  dcl-pi *n;",
    "    total packed(7:2);",
    "    note char(10) options(*nopass);",
    "  end-pi;",
    "  total *= 2;",
    "  if %parms() >= 2;",
    "    note = 'two';",
    "  endif;",
    "end-proc;",
]
M["PRC010"] = [
    "**FREE",
    "//; with: PRC010P",
    "//; setup: CRTBNDRPG PGM({lib}/PRC010P) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC010P) DBGVIEW(*NONE)",
    "//; cleanup: DLTPGM PGM({lib}/PRC010P)",
    "ctl-opt dftactgrp(*no);",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
    "dcl-pr callee extpgm('PRC010P');",
    "  t packed(7:2);",
    "  n char(10) options(*nopass);",
    "end-pr;",
    "dcl-s amt packed(7:2) inz(3.25);",
    "dcl-s txt char(10) inz('one');",
    "callee(amt : txt);",
] + p("%char(amt) + ' ' + txt") + ["callee(amt);"] + p("%char(amt) + ' ' + txt") + ["*inlr = *on;"]

# A cycle program with a global DCL-PI, and %PARMS for it.
INC["PRC011P"] = [
    "**FREE",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
    "dcl-pi *n;",
    "  a int(10);",
    "  b char(3) options(*nopass);",
    "end-pi;",
] + p("'got ' + %char(%parms())") + [
    "a += 1;",
    "if %parms() > 1;",
    "  b = 'yes';",
    "endif;",
    "*inlr = *on;",
]
M["PRC011"] = [
    "**FREE",
    "//; with: PRC011P",
    "//; setup: CRTBNDRPG PGM({lib}/PRC011P) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC011P) DBGVIEW(*NONE)",
    "//; cleanup: DLTPGM PGM({lib}/PRC011P)",
    "ctl-opt dftactgrp(*no);",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
    "dcl-pr callee extpgm('PRC011P');",
    "  a int(10);",
    "  b char(3) options(*nopass);",
    "end-pr;",
    "dcl-s n int(10) inz(41);",
    "dcl-s c char(3) inz('no');",
    "callee(n);",
] + p("%char(n) + ' ' + c") + ["callee(n : c);"] + p("%char(n) + ' ' + c") + ["*inlr = *on;"]

for name, lines in M.items():
    with open(os.path.join(OUT, name + ".rpgle"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
for name, lines in INC.items():
    with open(os.path.join(OUT, name + ".rpginc"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
print(len(M), "members,", len(INC), "brought members")
