"""Corpus batch 4, string, array and bit operations: fixed-form
members STRnnn, ARRnnn, BITnnn and the free-form built-in members.
Writes ../b04ops/{STR,ARR,BIT}nnn.rpgle.

usage: python -I b04s.py
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fixed import C, CX, D, PRT_D, PRT_F, show  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b04ops")
M = {}

FREE_HEAD = [
    "**FREE",
    "ctl-opt dftactgrp(*no);",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
]


def member(name, decls, body):
    M[name] = [PRT_F, PRT_D] + decls + body + [CX("EVAL", "*INLR = *ON")]


def free_member(name, decls, body):
    M[name] = FREE_HEAD + decls + body + ["*inlr = *on;"]


def p(expr):
    """Free form: print a character expression."""
    stmt = f"line = {expr};"
    out, cur = [], ""
    for part in stmt.split(" + "):
        cand = part if not cur else cur + " + " + part
        if len(cand) > 90 and cur:
            out.append(cur + " +")
            cur = "      " + part
        else:
            cur = cand
    out.append(cur)
    return "\n".join(out) + "\nwrite QSYSPRT line;"


def ev(x):
    return CX("EVAL", x)


def sc(label, fld):
    return show(f"'{label} [' + {fld} + ']'")


def sn(label, fld):
    return show(f"'{label} ' + %CHAR({fld})")


def inds(label, *ns):
    return show(f"'{label} ' + " + " + ".join(f"*IN{n}" for n in ns))


def S(name, length, typ="A", dec="", kw=""):
    return D(name, "S", to=str(length), typ=typ, dec=dec, kw=kw)


# CAT.
member("STR001", [
    S("R10", 10), S("C5", 5, kw="INZ('ab')"), S("V20", 20, kw="VARYING"),
    S("N", 3, "P", "0", "INZ(2)"),
], [
    ev("R10 = 'xxxxxxxxxx'"), C("'ab'", "CAT", "'cd'", "R10"), *sc("a", "R10"),
    ev("R10 = 'xxxxxxxxxx'"), C("'ab'", "CAT(P)", "'cd'", "R10"), *sc("b", "R10"),
    ev("R10 = 'xxxxxxxxxx'"), C("C5", "CAT", "'cd'", "R10"), *sc("c", "R10"),
    ev("R10 = 'xxxxxxxxxx'"), C("C5", "CAT", "'cd':1", "R10"), *sc("d", "R10"),
    ev("R10 = 'xxxxxxxxxx'"), C("C5", "CAT", "'cd':0", "R10"), *sc("e", "R10"),
    ev("R10 = 'hi'"), C(op="CAT", f2="'there':1", res="R10"), *sc("f", "R10"),
    ev("R10 = 'xxxxxxxxxx'"), C("'abcdefgh'", "CAT", "'ijklmn'", "R10"), *sc("g", "R10"),
    ev("V20 = 'x'"), C("'ab'", "CAT", "'cd'", "V20"),
    *show("'h [' + V20 + '] ' + %CHAR(%LEN(V20))"),
    ev("R10 = 'xxxxxxxxxx'"), C("'ab'", "CAT", "'cd':N", "R10"), *sc("i", "R10"),
    ev("R10 = 'xxxxxxxxxx'"), C("'ab'", "CAT", "'  cd':1", "R10"), *sc("j", "R10"),
    ev("R10 = 'xxxxxxxxxx'"), C("C5", "CAT(P)", "C5:2", "R10"), *sc("k", "R10"),
    ev("V20 = 'abc'"), C(op="CAT", f2="'de':1", res="V20"),
    *show("'l [' + V20 + '] ' + %CHAR(%LEN(V20))"),
    ev("R10 = 'xxxxxxxxxx'"), C("'abcdefghij'", "CAT", "'k':1", "R10"), *sc("m", "R10"),
])

# SCAN.
member("STR002", [
    S("S10", 10, kw="INZ('abcabcabc')"), S("POS", 3, "P", "0"),
    S("ARR", 3, "P", "0", "DIM(5)"), S("V", 10, kw="VARYING INZ('xxbc')"),
    S("ST", 3, "P", "0", "INZ(4)"),
], [
    C("'bc'", "SCAN", "S10", "POS", eq="50"), *show("'a ' + %CHAR(POS) + ' ' + *IN50"),
    C("'bc'", "SCAN", "S10:3", "POS", eq="50"), *show("'b ' + %CHAR(POS) + ' ' + *IN50"),
    C("'zz'", "SCAN", "S10", "POS", eq="50"), *show("'c ' + %CHAR(POS) + ' ' + *IN50"),
    C("'bcx':2", "SCAN", "S10", "POS"), *sn("d", "POS"),
    C("'b'", "SCAN", "S10", "ARR"),
    *show("'e ' + %CHAR(ARR(1)) + ' ' + %CHAR(ARR(2)) + ' ' + %CHAR(ARR(3)) + ' ' + "
          "%CHAR(ARR(4)) + ' ' + %CHAR(ARR(5))"),
    C("'c'", "SCAN", "S10", eq="51"), *show("'f ' + *IN51"),
    C("'q'", "SCAN", "S10", eq="51"), *show("'g ' + *IN51"),
    C("'bc'", "SCAN", "V", "POS"), *sn("h", "POS"),
    C("'a'", "SCAN", "S10:ST", "POS"), *sn("i", "POS"),
    C("' '", "SCAN", "S10", "POS"), *sn("j", "POS"),
    C("'c'", "SCAN", "S10:4", "ARR"),
    *show("'k ' + %CHAR(ARR(1)) + ' ' + %CHAR(ARR(2)) + ' ' + %CHAR(ARR(3))"),
])

# CHECK and CHECKR.
member("STR003", [
    S("DIG", 10, kw="INZ('0123456789')"), S("S", 8, kw="INZ('12a4b6  ')"),
    S("POS", 3, "P", "0"), S("ARR", 3, "P", "0", "DIM(4)"), S("T", 6, kw="INZ('  ab  ')"),
], [
    C("DIG", "CHECK", "S", "POS", eq="50"), *show("'a ' + %CHAR(POS) + ' ' + *IN50"),
    C("DIG", "CHECK", "S:4", "POS", eq="50"), *show("'b ' + %CHAR(POS) + ' ' + *IN50"),
    C("DIG", "CHECK", "'1234'", "POS", eq="50"), *show("'c ' + %CHAR(POS) + ' ' + *IN50"),
    C("DIG", "CHECK", "S", "ARR"),
    *show("'d ' + %CHAR(ARR(1)) + ' ' + %CHAR(ARR(2)) + ' ' + %CHAR(ARR(3)) + ' ' + %CHAR(ARR(4))"),
    C("' '", "CHECKR", "T", "POS", eq="51"), *show("'e ' + %CHAR(POS) + ' ' + *IN51"),
    C("' '", "CHECKR", "T:3", "POS", eq="51"), *show("'f ' + %CHAR(POS) + ' ' + *IN51"),
    C("' '", "CHECKR", "'   '", "POS", eq="51"), *show("'g ' + %CHAR(POS) + ' ' + *IN51"),
    C("DIG", "CHECKR", "S", "ARR"),
    *show("'h ' + %CHAR(ARR(1)) + ' ' + %CHAR(ARR(2)) + ' ' + %CHAR(ARR(3)) + ' ' + %CHAR(ARR(4))"),
    C("' '", "CHECK", "T", "POS"), *sn("i", "POS"),
    C("'ab '", "CHECK", "T", eq="52"), *show("'j ' + *IN52"),
])

# XLATE.
member("STR004", [
    S("R10", 10), S("R5", 5), S("S", 9, kw="INZ('aabbcc dd')"), S("FROM", 3, kw="INZ('abc')"),
    S("TO", 3, kw="INZ('XYZ')"),
], [
    ev("R10 = '**********'"), C("'abc':'ABC'", "XLATE", "S", "R10"), *sc("a", "R10"),
    ev("R10 = '**********'"), C("FROM:TO", "XLATE", "S:4", "R10"), *sc("b", "R10"),
    ev("R5 = '*****'"), C("FROM:TO", "XLATE", "S", "R5"), *sc("c", "R5"),
    ev("R10 = '**********'"), C("FROM:TO", "XLATE(P)", "'abc'", "R10"), *sc("d", "R10"),
    ev("R10 = '**********'"), C("FROM:TO", "XLATE", "'cab'", "R10"), *sc("e", "R10"),
    ev("R10 = '**********'"), C("'abc':'X'", "XLATE", "S", "R10"), *sc("f", "R10"),
    ev("R10 = '**********'"), C("' ':'_'", "XLATE", "S", "R10"), *sc("g", "R10"),
    ev("R10 = 'hello'"), C("'lo':'01'", "XLATE", "R10", "R10"), *sc("h", "R10"),
])

# SUBST.
member("STR005", [
    S("R5", 5), S("S", 8, kw="INZ('abcdefgh')"), S("L", 3, "P", "0", "INZ(2)"),
    S("ST", 3, "P", "0", "INZ(6)"), S("V", 10, kw="VARYING"),
], [
    ev("R5 = 'xxxxx'"), C("3", "SUBST", "S:2", "R5"), *sc("a", "R5"),
    ev("R5 = 'xxxxx'"), C("3", "SUBST(P)", "S:2", "R5"), *sc("b", "R5"),
    ev("R5 = 'xxxxx'"), C(op="SUBST", f2="S:6", res="R5"), *sc("c", "R5"),
    ev("R5 = 'xxxxx'"), C("L", "SUBST", "S:ST", "R5"), *sc("d", "R5"),
    ev("R5 = 'xxxxx'"), C("8", "SUBST", "S", "R5"), *sc("e", "R5"),
    ev("R5 = 'xxxxx'"), C("2", "SUBST", "S:1", "R5", lo="40"), *show("'h [' + R5 + '] ' + *IN40"),
    ev("V = 'abc'"), C("4", "SUBST", "S:3", "V"), *show("'i [' + V + '] ' + %CHAR(%LEN(V))"),
    ev("R5 = 'xxxxx'"), C(op="SUBST", f2="'pqrst':3", res="R5"), *sc("j", "R5"),
])

# EVALR.
member("STR006", [
    S("R10", 10), S("R3", 3), S("V", 10, kw="VARYING"),
], [
    CX("EVALR", "R10 = 'abc'"), *sc("a", "R10"),
    CX("EVALR", "R3 = 'abcdef'"), *sc("b", "R3"),
    CX("EVALR", "R10 = %TRIM('  xy  ')"), *sc("c", "R10"),
    CX("EVALR", "R10 = 'abc  '"), *sc("d", "R10"),
    ev("V = 'hello'"), CX("EVALR", "R10 = V"), *sc("e", "R10"),
    CX("EVALR", "R3 = V"), *sc("f", "R3"),
    CX("EVALR", "R10 = *BLANKS"), *sc("h", "R10"),
])

# BITON, BITOFF, TESTB.
member("BIT001", [
    D("B", "DS"), D("BC", frm="1", to="1"), D("BU", frm="1", to="1", typ="U", dec="0"),
    S("MASK", 1, kw="INZ(X'0F')"),
], [
    ev("BC = X'00'"), C(op="BITON", f2="'0'", res="BC"), *sn("a", "BU"),
    C(op="BITON", f2="'0167'", res="BC"), *sn("b", "BU"),
    C(op="BITOFF", f2="'0'", res="BC"), *sn("c", "BU"),
    C(op="BITON", f2="MASK", res="BC"), *sn("d", "BU"),
    C(op="BITOFF", f2="X'81'", res="BC"), *sn("e", "BU"),
    C(op="TESTB", f2="'01'", res="BC", hi="41", lo="42", eq="43"), *inds("f", 41, 42, 43),
    C(op="TESTB", f2="'12'", res="BC", hi="41", lo="42", eq="43"), *inds("g", 41, 42, 43),
    C(op="TESTB", f2="'67'", res="BC", hi="41", lo="42", eq="43"), *inds("h", 41, 42, 43),
    C(op="TESTB", f2="MASK", res="BC", hi="41", lo="42", eq="43"), *inds("i", 41, 42, 43),
    C(op="TESTB", f2="X'C0'", res="BC", hi="41", lo="42", eq="43"), *inds("j", 41, 42, 43),
    C(op="TESTB", f2="'7'", res="BC", eq="44"), *inds("k", 44),
    C(op="TESTB", f2="'0'", res="BC", hi="45"), *inds("l", 45),
    C(op="BITON", f2="'01234567'", res="BC"), *sn("m", "BU"),
    C(op="BITOFF", f2="'01234567'", res="BC"), *sn("n", "BU"),
])

# TESTN and TESTZ.
member("BIT002", [
    S("C5", 5),
], [
    *[x for v, l in [("'12345'", "a"), ("'  123'", "b"), ("'     '", "c"), ("'12 45'", "d"),
                     ("'1234J'", "e"), ("'ab123'", "f"), ("'123  '", "g")]
      for x in [ev(f"C5 = {v}"), C(op="TESTN", res="C5", hi="41", lo="42", eq="43"),
                *inds(l, 41, 42, 43)]],
    *[x for v, l in [("'A'", "h"), ("'J'", "i"), ("'1'", "j"), ("'&'", "k"), ("'-'", "l"),
                     ("'}'", "m"), ("'{'", "n"), ("'a'", "o")]
      for x in [ev(f"C5 = {v}"), C(op="TESTZ", res="C5", hi="41", lo="42", eq="43"),
                *inds(l, 41, 42, 43)]],
])

# COMP.
member("BIT003", [
    S("C3", 3, kw="INZ('abc')"), S("C5", 5, kw="INZ('abc')"), S("Z5", 5, "S", "2", "INZ(1.5)"),
    S("P3", 3, "P", "0", "INZ(2)"),
], [
    C("C3", "COMP", "C5", hi="41", lo="42", eq="43"), *inds("a", 41, 42, 43),
    C("C3", "COMP", "'abd'", hi="41", lo="42", eq="43"), *inds("b", 41, 42, 43),
    C("'b'", "COMP", "C5", hi="41", lo="42", eq="43"), *inds("c", 41, 42, 43),
    C("Z5", "COMP", "P3", hi="41", lo="42", eq="43"), *inds("d", 41, 42, 43),
    C("Z5", "COMP", "1.50", hi="41", lo="42", eq="43"), *inds("e", 41, 42, 43),
    C("P3", "COMP", "-5", hi="41", lo="42", eq="43"), *inds("f", 41, 42, 43),
    C("C3", "COMP", "*BLANKS", hi="41", lo="42", eq="43"), *inds("g", 41, 42, 43),
    C("C3", "COMP", "'abc'", eq="44"), *inds("h", 44),
    C("C3", "COMP", "'abc'", hi="45", lo="45"), *inds("i", 45),
    C("'A'", "COMP", "'a'", hi="41", lo="42", eq="43"), *inds("j", 41, 42, 43),
    C("'1'", "COMP", "'a'", hi="41", lo="42", eq="43"), *inds("k", 41, 42, 43),
])

# DIV with MVR, and SQRT.
member("BIT004", [
    S("Q", 5, "P", "0"), S("R", 5, "P", "0"), S("R2", 5, "P", "2"), S("Q1", 5, "P", "1"),
    S("SQ", 7, "P", "3"), S("SI", 5, "P", "0"), S("F", 8, "F"),
], [
    C("17", "DIV", "5", "Q"), C(op="MVR", res="R"), *show("'a ' + %CHAR(Q) + ' ' + %CHAR(R)"),
    C("-17", "DIV", "5", "Q"), C(op="MVR", res="R"), *show("'b ' + %CHAR(Q) + ' ' + %CHAR(R)"),
    C("10.5", "DIV", "4", "Q"), C(op="MVR", res="R2"), *show("'c ' + %CHAR(Q) + ' ' + %CHAR(R2)"),
    C("10", "DIV", "3", "Q1"), C(op="MVR", res="R2"), *show("'d ' + %CHAR(Q1) + ' ' + %CHAR(R2)"),
    C("17", "DIV", "-5", "Q"), C(op="MVR", res="R", hi="41", lo="42", eq="43"),
    *show("'e ' + %CHAR(R) + ' ' + *IN41 + *IN42 + *IN43"),
    C(op="SQRT", f2="2", res="SQ"), *sn("f", "SQ"),
    C(op="SQRT(H)", f2="2", res="SQ"), *sn("g", "SQ"),
    C(op="SQRT", f2="17", res="SI"), *sn("h", "SI"),
    C(op="SQRT(H)", f2="17", res="SI"), *sn("i", "SI"),
    C(op="SQRT", f2="144", res="SI"), *sn("j", "SI"),
    C(op="SQRT", f2="2", res="F"), *show("'k ' + %CHAR(%DEC(F:15:12))"),
    C(op="SQRT", f2="0.25", res="SQ"), *sn("l", "SQ"),
])

# LOOKUP on arrays.
member("ARR001", [
    S("ARR", 3, kw="DIM(5)"), S("NUM", 5, "P", "0", "DIM(4)"), S("I", 3, "P", "0"),
    S("J", 5, "I", "0"), S("K5", 5, "P", "0"),
], [
    ev("ARR(1) = 'aaa'"), ev("ARR(2) = 'bbb'"), ev("ARR(3) = 'ccc'"), ev("ARR(4) = 'bbb'"),
    ev("ARR(5) = 'eee'"),
    ev("NUM(1) = 7"), ev("NUM(2) = -3"), ev("NUM(3) = 12"), ev("NUM(4) = -3"),
    ev("I = 1"), C("'ccc'", "LOOKUP", "ARR(I)", eq="50"), *show("'a ' + %CHAR(I) + ' ' + *IN50"),
    ev("I = 1"), C("'zzz'", "LOOKUP", "ARR(I)", eq="50"), *show("'b ' + %CHAR(I) + ' ' + *IN50"),
    ev("I = 3"), C("'bbb'", "LOOKUP", "ARR(I)", eq="50"), *show("'c ' + %CHAR(I) + ' ' + *IN50"),
    C("'eee'", "LOOKUP", "ARR", eq="51"), *show("'d ' + *IN51"),
    C("'xxx'", "LOOKUP", "ARR", eq="51"), *show("'e ' + *IN51"),
    ev("K5 = -3"), ev("J = 1"), C("K5", "LOOKUP", "NUM(J)", eq="52"), *show("'f ' + %CHAR(J) + ' ' + *IN52"),
    ev("J = 3"), C("K5", "LOOKUP", "NUM(J)", eq="52"), *show("'g ' + %CHAR(J) + ' ' + *IN52"),
    ev("I = 4"), C("'aaa'", "LOOKUP", "ARR(I)", eq="50"), *show("'h ' + %CHAR(I) + ' ' + *IN50"),
    ev("I = 2"), C("'b  '", "LOOKUP", "ARR(I)", eq="50"), *show("'i ' + %CHAR(I) + ' ' + *IN50"),
    ev("K5 = 12"), C("K5", "LOOKUP", "NUM(3)", eq="53"), *show("'j ' + *IN53"),
])

# LOOKUP on sequenced arrays: high, low and equal.
member("ARR002", [
    S("ASC", 3, "P", "0", "DIM(5) ASCEND"), S("DSC", 3, "P", "0", "DIM(5) DESCEND"),
    S("I", 3, "P", "0"), S("CA", 2, kw="DIM(4) ASCEND"), S("K", 3, "P", "0"),
], [
    ev("ASC(1) = 10"), ev("ASC(2) = 20"), ev("ASC(3) = 30"), ev("ASC(4) = 40"), ev("ASC(5) = 50"),
    ev("DSC(1) = 50"), ev("DSC(2) = 40"), ev("DSC(3) = 30"), ev("DSC(4) = 20"), ev("DSC(5) = 10"),
    ev("CA(1) = 'b'"), ev("CA(2) = 'd'"), ev("CA(3) = 'f'"), ev("CA(4) = 'h'"),
    ev("I = 1"), ev("K = 25"), C("K", "LOOKUP", "ASC(I)", hi="51"), *show("'a ' + %CHAR(I) + ' ' + *IN51"),
    ev("I = 1"), ev("K = 25"), C("K", "LOOKUP", "ASC(I)", lo="52"), *show("'b ' + %CHAR(I) + ' ' + *IN52"),
    ev("I = 1"), ev("K = 30"), C("K", "LOOKUP", "ASC(I)", hi="51", eq="53"),
    *show("'c ' + %CHAR(I) + ' ' + *IN51 + *IN53"),
    ev("I = 1"), ev("K = 30"), C("K", "LOOKUP", "ASC(I)", lo="52", eq="53"),
    *show("'d ' + %CHAR(I) + ' ' + *IN52 + *IN53"),
    ev("I = 1"), ev("K = 60"), C("K", "LOOKUP", "ASC(I)", hi="51"), *show("'e ' + %CHAR(I) + ' ' + *IN51"),
    ev("I = 1"), ev("K = 5"), C("K", "LOOKUP", "ASC(I)", lo="52"), *show("'f ' + %CHAR(I) + ' ' + *IN52"),
    ev("I = 1"), ev("K = 25"), C("K", "LOOKUP", "DSC(I)", hi="51"), *show("'g ' + %CHAR(I) + ' ' + *IN51"),
    ev("I = 1"), ev("K = 25"), C("K", "LOOKUP", "DSC(I)", lo="52"), *show("'h ' + %CHAR(I) + ' ' + *IN52"),
    ev("I = 1"), C("'e '", "LOOKUP", "CA(I)", hi="51"), *show("'i ' + %CHAR(I) + ' ' + *IN51"),
    ev("I = 1"), C("'e '", "LOOKUP", "CA(I)", lo="52"), *show("'j ' + %CHAR(I) + ' ' + *IN52"),
    ev("I = 3"), ev("K = 25"), C("K", "LOOKUP", "ASC(I)", lo="52"), *show("'k ' + %CHAR(I) + ' ' + *IN52"),
    ev("I = 1"), ev("K = 40"), C("K", "LOOKUP", "ASC(I)", eq="53"), *show("'l ' + %CHAR(I) + ' ' + *IN53"),
])

# LOOKUP on tables, with alternate tables.
M["ARR003"] = [
    PRT_F, PRT_D,
    D("TABA", "S", to="3", kw="DIM(4) CTDATA PERRCD(1)"),
    D("TABB", "S", to="5", kw="DIM(4) ALT(TABA)"),
    D("TABN", "S", to="3", typ="S", dec="0", kw="DIM(4) CTDATA PERRCD(2)"),
    S("X", 3), S("K3", 3, "S", "0", "INZ(20)"),
    ev("X = TABA"), *sc("a", "X"),
    C("'CCC'", "LOOKUP", "TABA", "TABB", eq="50"),
    *show("'b ' + *IN50 + ' ' + TABA + ' ' + TABB"),
    C("'ZZZ'", "LOOKUP", "TABA", "TABB", eq="50"),
    *show("'c ' + *IN50 + ' ' + TABA + ' ' + TABB"),
    C("'AAA'", "LOOKUP", "TABA", eq="50"),
    *show("'d ' + *IN50 + ' ' + TABA + ' ' + TABB"),
    C("'BBB'", "LOOKUP", "TABA", "TABB", eq="50"),
    ev("TABB = 'new'"),
    C("'BBB'", "LOOKUP", "TABA", "TABB", eq="50"),
    *show("'e ' + *IN50 + ' ' + TABA + ' ' + TABB"),
    C("K3", "LOOKUP", "TABN", eq="51"), *show("'f ' + *IN51 + ' ' + %CHAR(TABN)"),
    CX("EVAL", "*INLR = *ON"),
    "**CTDATA TABA",
    "AAAone",
    "BBBtwo",
    "CCCthree",
    "DDDfour",
    "**CTDATA TABN",
    "010020",
    "030040",
]

# XFOOT.
member("ARR004", [
    S("NUM", 5, "P", "2", "DIM(4)"), S("SUM", 7, "P", "2"), S("SMALL", 3, "P", "1"),
    S("IA", 10, "I", "0", "DIM(3)"), S("ISUM", 10, "I", "0"),
], [
    ev("NUM(1) = 1.25"), ev("NUM(2) = 2.50"), ev("NUM(3) = -0.75"), ev("NUM(4) = 10"),
    C(op="XFOOT", f2="NUM", res="SUM", hi="41", lo="42", eq="43"),
    *show("'a ' + %CHAR(SUM) + ' ' + *IN41 + *IN42 + *IN43"),
    C(op="XFOOT(H)", f2="NUM", res="SMALL"), *sn("b", "SMALL"),
    C(op="XFOOT", f2="NUM", res="SMALL"), *sn("c", "SMALL"),
    C(op="XFOOT", f2="NUM", res="NUM(1)"), *sn("d", "NUM(1)"),
    ev("NUM = 0"), C(op="XFOOT", f2="NUM", res="SUM", hi="41", lo="42", eq="43"),
    *show("'e ' + %CHAR(SUM) + ' ' + *IN41 + *IN42 + *IN43"),
    ev("IA(1) = 5"), ev("IA(2) = -12"), ev("IA(3) = 4"),
    C(op="XFOOT", f2="IA", res="ISUM", hi="41", lo="42", eq="43"),
    *show("'f ' + %CHAR(ISUM) + ' ' + *IN41 + *IN42 + *IN43"),
    *show("'g ' + %CHAR(%XFOOT(NUM)) + ' ' + %CHAR(%XFOOT(IA))"),
])


import b04s_free  # noqa: E402

b04s_free.add(M, member, free_member, p, ev, sn, S, FREE_HEAD)


def main():
    for name, lines in M.items():
        text = "\n".join(lines) + "\n"
        for i, l in enumerate(text.splitlines(), 1):
            if len(l) > 100:
                raise SystemExit(f"{name} line {i} is {len(l)} wide")
        with open(os.path.join(OUT, name + ".rpgle"), "w", newline="\n") as f:
            f.write(text)
    print(f"{len(M)} members")


if __name__ == "__main__":
    main()
