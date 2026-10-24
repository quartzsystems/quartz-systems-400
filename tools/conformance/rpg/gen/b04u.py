"""Corpus batch 4, UCS-2: fields, U'...' literals, assignment between
character and UCS-2 both ways, concatenation and comparison across the
classes, %UCS2, %CHAR, %LEN, %SIZE, %TRIM, %SUBST and %SCAN on UCS-2.
Writes ../b04ops/UCSnnn.rpgle.

usage: python -I b04u.py
"""
import os

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b04ops")
HEAD = ["**FREE", "ctl-opt dftactgrp(*no);", "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"]
M = {}


def p(expr):
    return [f"line = {expr};", "write QSYSPRT line;"]


M["UCS001"] = HEAD + [
    "dcl-s u ucs2(10) inz('Quartz');",
    "dcl-s v varucs2(20);",
    "dcl-s c char(12);",
    "dcl-s n int(10);",
    "c = u;",
] + p("'[' + c + ']'") + [
    "v = %trim(u) + ' sys';",
] + p("'[' + %char(v) + '] ' + %char(%len(v))") + [
    "v = U'00410042';",
] + p("%char(v) + ' ' + %char(%len(v)) + ' ' + %char(%size(u))") + [
    "u = 'ab';",
    "n = %scan(%ucs2('b') : u);",
] + p("'scan ' + %char(n)") + p("'sub [' + %char(%subst(u : 2 : 3)) + ']'") + [
    "*inlr = *on;",
]

M["UCS002"] = HEAD + [
    "dcl-s u ucs2(5) inz('abc');",
    "dcl-s w ucs2(5) inz('abd');",
    "dcl-s c char(5) inz('abc');",
] + p("'eq ' + %char(u = c)") + p("'lt ' + %char(u < w)") + p("'lit ' + %char(u = 'abc')") + p("'mix [' + %char(c + u) + ']'") + [
    "*inlr = *on;",
]

for name, lines in M.items():
    with open(os.path.join(OUT, name + ".rpgle"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
print(len(M), "members")
