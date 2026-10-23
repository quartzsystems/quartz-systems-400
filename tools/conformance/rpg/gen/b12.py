"""Corpus batch 12: compile errors. Each member carries one mistake (a
few carry two that interact) and only compiles; the oracle's message
IDs, severities and statements are what the kit compares.

usage: python -I b12.py
"""
import os

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b12err")
HEAD = ["**FREE", "//; run: no", "ctl-opt dftactgrp(*no);"]

# Each case: the declarations and statements after the heading.
CASES = [
    # Names.
    ["x = 1;"],
    ["dcl-s a int(10);", "dcl-s a int(10);"],
    ["dcl-s a like(nothere);"],
    ["dcl-ds d likeds(nods);", "end-ds;"],
    ["dcl-s a int(10);", "a = b + 1;"],
    ["exsr nosuch;"],
    ["dcl-s a int(10);", "a = undefproc(1);"],
    # Types and lengths.
    ["dcl-s a packed(64:0);"],
    ["dcl-s a packed(5:6);"],
    ["dcl-s a char(0);"],
    ["dcl-s a int(7);"],
    ["dcl-s a uns(12);"],
    ["dcl-s a float(6);"],
    ["dcl-s a zoned(0:0);"],
    ["dcl-s a varchar(16773105);"],
    ["dcl-s a char(10) dim(0);"],
    ["dcl-s a char(10) dim(-1);"],
    ["dcl-s a date(*xyz);"],
    ["dcl-s a ind inz('2');"],
    ["dcl-s a packed(3:0) inz(12345);"],
    ["dcl-s a char(3) inz('abcdef');"],
    ["dcl-s a int(10) inz('abc');"],
    ["dcl-s a date inz(d'2026-13-01');"],
    ["dcl-s a time inz(t'25.00.00');"],
    # Keywords.
    ["dcl-s a int(10) nosuchkw;"],
    ["dcl-s a int(10) const;"],
    ["dcl-s a int(10) export import;"],
    ["dcl-s a int(10) dim(3) dim(4);"],
    ["dcl-c k;"],
    ["dcl-c k 'a' inz('b');"],
    ["ctl-opt nosuchopt;"],
    ["ctl-opt datfmt(*xyz);"],
    ["ctl-opt dftactgrp(*yes) actgrp('X');"],
    # Expressions.
    ["dcl-s a int(10);", "a = 'abc';"],
    ["dcl-s c char(5);", "c = 12;"],
    ["dcl-s a int(10);", "dcl-s c char(5);", "a = c + 1;"],
    ["dcl-s f ind;", "f = 3;"],
    ["dcl-s a int(10);", "a = (1 + 2;"],
    ["dcl-s a int(10);", "a = 1 +;"],
    ["dcl-s a int(10);", "a = 1 2;"],
    ["dcl-s d date;", "dcl-s a int(10);", "a = d + 1;"],
    ["dcl-s a int(10);", "if a;", "endif;"],
    ["dcl-s a char(5);", "a = %subst(a);"],
    ["dcl-s a char(5);", "a = %subst(a : 'x');"],
    ["dcl-s a int(10);", "a = %len();"],
    ["dcl-s a int(10);", "a = %nosuch(1);"],
    ["dcl-s a int(10);", "a = %int('x' : 2);"],
    ["dcl-s a int(10);", "a = %elem(a);"],
    ["dcl-s a int(10);", "a = 1 / 0;"],
    ["dcl-s a char(5) dim(3);", "a = 'x' + a;"],
    # Structure.
    ["if 1 = 1;"],
    ["endif;"],
    ["dow 1 = 1;", "enddo;", "enddo;"],
    ["leave;"],
    ["iter;"],
    ["select;", "other;", "when 1 = 1;", "endsl;"],
    ["for i = 1 to 3;", "endfor;"],
    ["dcl-s a int(10)", "a = 1;"],
    ["begsr s1;", "endsr;", "begsr s1;", "endsr;"],
    ["begsr s1;", "begsr s2;", "endsr;", "endsr;"],
    ["monitor;", "on-error;", "endmon;"],
    ["on-error;"],
    # Procedures.
    ["dcl-proc p;", "  return 1;", "end-proc;"],
    ["dcl-proc p;", "  dcl-pi *n int(10);", "  end-pi;", "  return;", "end-proc;"],
    ["dcl-proc p;", "  dcl-pi *n;", "    x int(10) value;", "  end-pi;", "end-proc;", "p();"],
    ["dcl-proc p;", "  dcl-pi *n;", "    x int(10);", "  end-pi;", "end-proc;", "p(5);"],
    ["dcl-proc p;", "  dcl-pi *n;", "  end-pi;", "end-proc;", "p(1 : 2);"],
    ["dcl-pr q int(10);", "  x int(10) value;", "end-pr;", "dcl-proc q;", "  dcl-pi *n int(10);", "    x char(5) value;", "  end-pi;", "  return 1;", "end-proc;"],
    ["dcl-proc p;", "  dcl-pi *n;", "  end-pi;"],
    ["end-proc;"],
    ["dcl-pi *n;", "end-pi;", "dcl-pi *n;", "end-pi;"],
    ["dcl-ds d;", "  a int(10);"],
    ["dcl-ds d qualified;", "  a int(10);", "end-ds;", "a = 1;"],
    ["dcl-ds d;", "  a int(10) pos(0);", "end-ds;"],
    ["dcl-ds d len(2);", "  a int(10);", "end-ds;"],
    ["dcl-ds d;", "  a int(10) overlay(nosuch);", "end-ds;"],
    ["dcl-ds d dim(2);", "  a int(10);", "end-ds;", "d.a = 1;"],
    # Files.
    ["dcl-f nofile disk;"],
    ["dcl-f QSYSPRT printer(132);", "read QSYSPRT;"],
    ["dcl-f QSYSPRT printer(132) usage(*input);"],
    ["dcl-f QSYSPRT printer(132);", "dcl-f QSYSPRT printer(132);"],
    ["chain 1 nofile;"],
    # Found by batch 2 (appended, so the numbers above stay).
    ["dcl-s a bindec(13:3);"],
    ["dcl-s f float(8);", "dcl-s a int(10);", "a = %decpos(f);"],
    ["dcl-s n char(10);", "dcl-s m like(n : 5);"],
    ["dcl-ds d len(40);", "  p char(2) overlay(d : 30);", "end-ds;"],
    ["dcl-c K *blanks;", "dcl-s c char(5);", "c = 'a' + K;"],
    ["FIXED",
     "     H DFTACTGRP(*NO)",
     "     D c               S             10",
     "      /free",
     "        c = 'short';                                                            c = 'cut';",
     "        *inlr = *on;",
     "      /end-free"],
    ["CMD", "//; cmd: CRTRPGMOD MODULE({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) OUTPUT(*PRINT)",
     "ctl-opt dftactgrp(*no);", "dcl-s a int(10);"],
]

for i, body in enumerate(CASES, 1):
    name = f"ERR{i:03d}"
    with open(os.path.join(OUT, name + ".rpgle"), "w", encoding="utf-8", newline="\n") as f:
        if body[0] == "CMD":
            f.write("\n".join(["**FREE", "//; run: no", body[1]] + body[2:] + ["*inlr = *on;"]) + "\n")
        elif body[0] == "FIXED":
            f.write("\n".join(["     *; run: no"] + body[1:]) + "\n")
        else:
            f.write("\n".join(HEAD + body + ["*inlr = *on;"]) + "\n")
print(len(CASES), "members")
