"""Corpus batch 5: the MOVE, MOVEL and MOVEA matrix, fixed form.
Writes ../b05move/MOVnnn.rpgle.

usage: python -I b05.py
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fixed import C, CX, D, PRT_D, PRT_F, show  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b05move")
M = {}


def member(name, decls, body):
    lines = [PRT_F, PRT_D] + decls + body + [CX("EVAL", "*INLR = *ON")]
    M[name] = lines


def ev(x):
    return CX("EVAL", x)


def mv(op, src, tgt, init=None):
    """Optionally set the target, then move."""
    out = []
    if init is not None:
        out.append(ev(f"{tgt} = {init}"))
    out.append(C(op=op, f2=src, res=tgt))
    return out


def sc(label, fld):
    """Print a character field between brackets."""
    return show(f"'{label} [' + {fld} + ']'")


def sn(label, fld):
    """Print a number as %CHAR and as its zoned digits."""
    return show(f"'{label} ' + %CHAR({fld}) + ' ' + %EDITC({fld}:'X')")


def S(name, length, typ="A", dec="", kw=""):
    return D(name, "S", to=str(length), typ=typ, dec=dec, kw=kw)


# 1: character to character, MOVE.
member("MOV001", [
    S("C3", 3), S("C5", 5), S("C8", 8, kw="INZ('12345678')"),
], [
    *mv("MOVE", "'XY'", "C5", "'abcde'"), *sc("a", "C5"),
    *mv("MOVE(P)", "'XY'", "C5", "'abcde'"), *sc("b", "C5"),
    *mv("MOVE", "C8", "C3", "'abc'"), *sc("c", "C3"),
    *mv("MOVE(P)", "C8", "C3", "'abc'"), *sc("d", "C3"),
    *mv("MOVE", "C5", "C8"), *sc("e", "C8"),
    *mv("MOVE", "'Q'", "C3", "'abc'"), *sc("f", "C3"),
    *mv("MOVE", "C3", "C3"), *sc("g", "C3"),
    *mv("MOVE(P)", "'LONGERTEXT'", "C5", "'abcde'"), *sc("h", "C5"),
])

# 2: character to character, MOVEL.
member("MOV002", [
    S("C3", 3), S("C5", 5), S("C8", 8, kw="INZ('12345678')"),
], [
    *mv("MOVEL", "'XY'", "C5", "'abcde'"), *sc("a", "C5"),
    *mv("MOVEL(P)", "'XY'", "C5", "'abcde'"), *sc("b", "C5"),
    *mv("MOVEL", "C8", "C3", "'abc'"), *sc("c", "C3"),
    *mv("MOVEL(P)", "C8", "C3", "'abc'"), *sc("d", "C3"),
    *mv("MOVEL", "C5", "C8"), *sc("e", "C8"),
    *mv("MOVEL(P)", "C3", "C8"), *sc("f", "C8"),
    *mv("MOVEL", "'LONGERTEXT'", "C5", "'abcde'"), *sc("g", "C5"),
])

# 3: zoned to character, MOVE.
member("MOV003", [
    S("C3", 3), S("C6", 6), S("C10", 10),
    S("Z5", 5, "S", "0", "INZ(12345)"), S("ZN", 5, "S", "0", "INZ(-12345)"),
    S("Z72", 7, "S", "2", "INZ(-123.45)"), S("Z3", 3, "S", "0", "INZ(-120)"),
], [
    *mv("MOVE", "Z5", "C6", "'abcdef'"), *sc("a", "C6"),
    *mv("MOVE", "ZN", "C6", "'abcdef'"), *sc("b", "C6"),
    *mv("MOVE", "Z5", "C3", "'abc'"), *sc("c", "C3"),
    *mv("MOVE", "ZN", "C3", "'abc'"), *sc("d", "C3"),
    *mv("MOVE(P)", "ZN", "C10", "'abcdefghij'"), *sc("e", "C10"),
    *mv("MOVE", "Z72", "C10", "'abcdefghij'"), *sc("f", "C10"),
    *mv("MOVE", "Z3", "C3", "'abc'"), *sc("g", "C3"),
    *mv("MOVE(P)", "Z72", "C3", "'abc'"), *sc("h", "C3"),
])

# 4: zoned to character, MOVEL.
member("MOV004", [
    S("C3", 3), S("C6", 6), S("C10", 10),
    S("Z5", 5, "S", "0", "INZ(12345)"), S("ZN", 5, "S", "0", "INZ(-12345)"),
    S("Z72", 7, "S", "2", "INZ(-123.45)"),
], [
    *mv("MOVEL", "Z5", "C6", "'abcdef'"), *sc("a", "C6"),
    *mv("MOVEL", "ZN", "C6", "'abcdef'"), *sc("b", "C6"),
    *mv("MOVEL", "Z5", "C3", "'abc'"), *sc("c", "C3"),
    *mv("MOVEL", "ZN", "C3", "'abc'"), *sc("d", "C3"),
    *mv("MOVEL(P)", "ZN", "C10", "'abcdefghij'"), *sc("e", "C10"),
    *mv("MOVEL", "Z72", "C10", "'abcdefghij'"), *sc("f", "C10"),
    *mv("MOVEL(P)", "Z72", "C3", "'abc'"), *sc("g", "C3"),
])

# 5: packed, binary and integer to character.
member("MOV005", [
    S("C4", 4), S("C8", 8), S("C12", 12),
    S("P7", 7, "P", "2", "INZ(-12345.67)"), S("P5", 5, "P", "0", "INZ(678)"),
    S("B4", 4, "B", "0", "INZ(-42)"), S("I10", 10, "I", "0", "INZ(-1234567)"),
    S("U5", 5, "U", "0", "INZ(65000)"), S("I3", 3, "I", "0", "INZ(7)"),
], [
    *mv("MOVE", "P7", "C8", "'abcdefgh'"), *sc("a", "C8"),
    *mv("MOVEL", "P7", "C4", "'abcd'"), *sc("b", "C4"),
    *mv("MOVE", "P5", "C8", "'abcdefgh'"), *sc("c", "C8"),
    *mv("MOVE", "B4", "C8", "'abcdefgh'"), *sc("d", "C8"),
    *mv("MOVE", "I10", "C12", "'abcdefghijkl'"), *sc("e", "C12"),
    *mv("MOVEL", "I10", "C4", "'abcd'"), *sc("f", "C4"),
    *mv("MOVE", "U5", "C8", "'abcdefgh'"), *sc("g", "C8"),
    *mv("MOVE(P)", "I3", "C8", "'abcdefgh'"), *sc("h", "C8"),
    *mv("MOVEL(P)", "B4", "C8", "'abcdefgh'"), *sc("i", "C8"),
])

# 6: character to zoned, MOVE.
member("MOV006", [
    S("Z5", 5, "S", "0"), S("Z3", 3, "S", "0"), S("Z72", 7, "S", "2"),
    S("C3", 3, kw="INZ('123')"), S("C5", 5, kw="INZ('9876J')"), S("CA", 4, kw="INZ('ABCD')"),
    S("CB", 3, kw="INZ(' 12')"), S("C8", 8, kw="INZ('12345678')"), S("CL", 3, kw="INZ('12}')"),
], [
    *mv("MOVE", "C3", "Z5", "55555"), *sn("a", "Z5"),
    *mv("MOVE(P)", "C3", "Z5", "55555"), *sn("b", "Z5"),
    *mv("MOVE", "C5", "Z5", "55555"), *sn("c", "Z5"),
    *mv("MOVE", "CA", "Z5", "55555"), *sn("d", "Z5"),
    *mv("MOVE", "CB", "Z5", "55555"), *sn("e", "Z5"),
    *mv("MOVE", "C8", "Z3", "555"), *sn("f", "Z3"),
    *mv("MOVE", "CL", "Z72", "-11111.11"), *sn("g", "Z72"),
    *mv("MOVE", "C3", "Z5", "-55555"), *sn("h", "Z5"),
    *mv("MOVE", "'4M'", "Z5", "55555"), *sn("i", "Z5"),
])

# 7: character to zoned, MOVEL.
member("MOV007", [
    S("Z5", 5, "S", "0"), S("Z3", 3, "S", "0"), S("Z72", 7, "S", "2"),
    S("C3", 3, kw="INZ('123')"), S("C5", 5, kw="INZ('9876J')"), S("CN", 3, kw="INZ('12L')"),
    S("C8", 8, kw="INZ('1234567R')"),
], [
    *mv("MOVEL", "C3", "Z5", "55555"), *sn("a", "Z5"),
    *mv("MOVEL", "C3", "Z5", "-55555"), *sn("b", "Z5"),
    *mv("MOVEL(P)", "C3", "Z5", "-55555"), *sn("c", "Z5"),
    *mv("MOVEL", "C5", "Z5", "55555"), *sn("d", "Z5"),
    *mv("MOVEL", "CN", "Z5", "55555"), *sn("e", "Z5"),
    *mv("MOVEL(P)", "CN", "Z5", "55555"), *sn("f", "Z5"),
    *mv("MOVEL", "C8", "Z3", "555"), *sn("g", "Z3"),
    *mv("MOVEL", "C8", "Z72", "11111.11"), *sn("h", "Z72"),
    *mv("MOVEL", "CN", "Z3", "555"), *sn("i", "Z3"),
])

# 8: character to packed, binary and integer.
member("MOV008", [
    S("P5", 5, "P", "0"), S("P72", 7, "P", "2"), S("B4", 4, "B", "0"),
    S("I10", 10, "I", "0"), S("U5", 5, "U", "0"),
    S("C3", 3, kw="INZ('123')"), S("CN", 3, kw="INZ('12L')"), S("C8", 8, kw="INZ('87654321')"),
], [
    *mv("MOVE", "C3", "P5", "55555"), *sn("a", "P5"),
    *mv("MOVE", "CN", "P5", "55555"), *sn("b", "P5"),
    *mv("MOVEL", "C3", "P5", "-55555"), *sn("c", "P5"),
    *mv("MOVE", "C8", "P72", "0"), *sn("d", "P72"),
    *mv("MOVE", "CN", "B4", "5555"), *sn("e", "B4"),
    *mv("MOVE", "C8", "I10", "0"), *sn("f", "I10"),
    *mv("MOVEL", "C3", "I10", "1111111111"), *sn("g", "I10"),
    *mv("MOVE", "C3", "U5", "55555"), *sn("h", "U5"),
    *mv("MOVE(P)", "CN", "P72", "11111.11"), *sn("i", "P72"),
])

# 9: numeric to numeric, MOVE.
member("MOV009", [
    S("Z5", 5, "S", "0", "INZ(12345)"), S("ZN", 5, "S", "0", "INZ(-12345)"),
    S("Z3", 3, "S", "0"), S("Z7", 7, "S", "0"), S("Z72", 7, "S", "2"),
    S("Z42", 4, "S", "2", "INZ(-98.76)"), S("P9", 9, "P", "3"),
], [
    *mv("MOVE", "Z5", "Z3", "999"), *sn("a", "Z3"),
    *mv("MOVE", "ZN", "Z3", "999"), *sn("b", "Z3"),
    *mv("MOVE", "Z5", "Z7", "9999999"), *sn("c", "Z7"),
    *mv("MOVE", "ZN", "Z7", "9999999"), *sn("d", "Z7"),
    *mv("MOVE(P)", "ZN", "Z7", "9999999"), *sn("e", "Z7"),
    *mv("MOVE", "Z5", "Z72", "-99999.99"), *sn("f", "Z72"),
    *mv("MOVE", "Z42", "Z72", "11111.11"), *sn("g", "Z72"),
    *mv("MOVE", "Z42", "Z5", "55555"), *sn("h", "Z5"),
    *mv("MOVE", "Z42", "P9", "-123456.789"), *sn("i", "P9"),
    *mv("MOVE", "Z7", "Z7"), *sn("j", "Z7"),
])

# 10: numeric to numeric, MOVEL.
member("MOV010", [
    S("Z5", 5, "S", "0", "INZ(12345)"), S("ZN", 5, "S", "0", "INZ(-12345)"),
    S("Z3", 3, "S", "0"), S("Z7", 7, "S", "0"), S("Z72", 7, "S", "2"),
    S("Z42", 4, "S", "2", "INZ(-98.76)"), S("P9", 9, "P", "3"),
], [
    *mv("MOVEL", "Z5", "Z3", "999"), *sn("a", "Z3"),
    *mv("MOVEL", "ZN", "Z3", "999"), *sn("b", "Z3"),
    *mv("MOVEL", "Z5", "Z7", "9999999"), *sn("c", "Z7"),
    *mv("MOVEL", "ZN", "Z7", "9999999"), *sn("d", "Z7"),
    *mv("MOVEL", "Z5", "Z7", "-9999999"), *sn("e", "Z7"),
    *mv("MOVEL(P)", "ZN", "Z7", "9999999"), *sn("f", "Z7"),
    *mv("MOVEL(P)", "Z5", "Z7", "-9999999"), *sn("g", "Z7"),
    *mv("MOVEL", "Z42", "Z72", "11111.11"), *sn("h", "Z72"),
    *mv("MOVEL", "Z42", "P9", "-123456.789"), *sn("i", "P9"),
    *mv("MOVEL", "ZN", "Z42", "11.11"), *sn("j", "Z42"),
])

# 11: packed, binary and integer between themselves.
member("MOV011", [
    S("P5", 5, "P", "0", "INZ(-12345)"), S("P3", 3, "P", "1"), S("P9", 9, "P", "0"),
    S("B4", 4, "B", "0"), S("I5", 5, "I", "0"), S("I10", 10, "I", "0"),
    S("U3", 3, "U", "0"), S("I20", 20, "I", "0", "INZ(-98765432109876)"),
], [
    *mv("MOVE", "P5", "P3", "99.9"), *sn("a", "P3"),
    *mv("MOVEL", "P5", "P3", "99.9"), *sn("b", "P3"),
    *mv("MOVE", "P5", "B4", "1111"), *sn("c", "B4"),
    *mv("MOVE", "P5", "I5", "11111"), *sn("e", "I5"),
    *mv("MOVEL", "P5", "I10", "1111111111"), *sn("f", "I10"),
    *mv("MOVE", "I20", "P9", "0"), *sn("h", "P9"),
    *mv("MOVEL", "I20", "P9", "0"), *sn("i", "P9"),
    *mv("MOVE(P)", "P3", "I10", "1111111111"), *sn("j", "I10"),
])

# 12: literals and figurative constants.
member("MOV012", [
    S("C5", 5), S("Z5", 5, "S", "0"), S("P72", 7, "P", "2"), S("C8", 8),
], [
    *mv("MOVE", "123", "C5", "'abcde'"), *sc("a", "C5"),
    *mv("MOVE", "-123", "C5", "'abcde'"), *sc("b", "C5"),
    *mv("MOVEL", "-1.5", "C5", "'abcde'"), *sc("c", "C5"),
    *mv("MOVE", "*BLANKS", "C5", "'abcde'"), *sc("d", "C5"),
    *mv("MOVE", "*ZEROS", "C5", "'abcde'"), *sc("e", "C5"),
    *mv("MOVE", "*ALL'xy'", "C5", "'abcde'"), *sc("f", "C5"),
    *mv("MOVEL", "*ALL'xy'", "C5", "'abcde'"), *sc("g", "C5"),
    *mv("MOVE", "'77'", "Z5", "12345"), *sn("h", "Z5"),
    *mv("MOVE", "*ZEROS", "Z5", "12345"), *sn("i", "Z5"),
    *mv("MOVE", "*ALL'3'", "Z5", "12345"), *sn("j", "Z5"),
    *mv("MOVE", "-42", "P72", "11111.11"), *sn("k", "P72"),
    *mv("MOVEL", "42", "P72", "11111.11"), *sn("l", "P72"),
    *mv("MOVE", "*HIVAL", "Z5", "1"), *sn("m", "Z5"),
    *mv("MOVE", "*LOVAL", "Z5", "1"), *sn("n", "Z5"),
    *mv("MOVE", "*BLANKS", "Z5", "12345"), *sn("o", "Z5"),
    *mv("MOVEL", "'ab'", "C8", "'12345678'"), *sc("p", "C8"),
])

# 13: varying fields.
member("MOV013", [
    S("V8", 8, kw="VARYING"), S("V3", 3, kw="VARYING"), S("C5", 5), S("Z5", 5, "S", "0"),
], [
    ev("V8 = 'abc'"), *mv("MOVE", "'XY'", "V8"),
    *show("'a [' + V8 + '] ' + %CHAR(%LEN(V8))"),
    ev("V8 = 'abcdef'"), *mv("MOVEL", "'XY'", "V8"),
    *show("'b [' + V8 + '] ' + %CHAR(%LEN(V8))"),
    ev("V8 = 'abcdef'"), *mv("MOVE(P)", "'XY'", "V8"),
    *show("'c [' + V8 + '] ' + %CHAR(%LEN(V8))"),
    ev("V8 = 'abcdef'"), *mv("MOVEL(P)", "'XY'", "V8"),
    *show("'d [' + V8 + '] ' + %CHAR(%LEN(V8))"),
    ev("V3 = 'pq'"), *mv("MOVE", "V3", "C5", "'abcde'"), *sc("e", "C5"),
    ev("V3 = 'pq'"), *mv("MOVEL", "V3", "C5", "'abcde'"), *sc("f", "C5"),
    ev("V3 = ''"), *mv("MOVE", "V3", "C5", "'abcde'"), *sc("g", "C5"),
    ev("V3 = '12'"), *mv("MOVE", "V3", "Z5", "55555"), *sn("h", "Z5"),
    ev("V8 = 'abcd'"), *mv("MOVE", "Z5", "V8"),
    *show("'i [' + V8 + '] ' + %CHAR(%LEN(V8))"),
    ev("V8 = ''"), *mv("MOVE", "'XY'", "V8"),
    *show("'j [' + V8 + '] ' + %CHAR(%LEN(V8))"),
])

# 14: indicators.
member("MOV014", [
    S("C1", 1), S("C3", 3), S("Z1", 1, "S", "0"), S("N1", 1, "N"),
], [
    *mv("MOVE", "'1'", "*IN01"), *mv("MOVE", "'0'", "*IN02"),
    *show("'a ' + *IN01 + *IN02"),
    *mv("MOVE", "*IN01", "C1", "'x'"), *sc("b", "C1"),
    *mv("MOVE", "*IN01", "C3", "'xyz'"), *sc("c", "C3"),
    *mv("MOVEL", "*IN02", "C3", "'xyz'"), *sc("d", "C3"),
    *mv("MOVE", "*ON", "N1"), *show("'e ' + N1"),
    *mv("MOVE", "*OFF", "*IN01"), *show("'f ' + *IN01"),
    *mv("MOVE", "N1", "Z1", "5"), *sn("i", "Z1"),
])

# 15: structures, subfields and array elements.
member("MOV015", [
    D("DS1", "DS"), D("SA", frm="1", to="3"), D("SZ", frm="4", to="8", typ="S", dec="0"),
    D("SB", frm="9", to="10"),
    S("ARR", 4, kw="DIM(3)"), S("I", 3, "P", "0", "INZ(2)"),
    S("ZA", 3, "S", "0", "DIM(2)"),
], [
    ev("DS1 = 'abc00042xy'"),
    *mv("MOVE", "'Q'", "DS1"), *sc("a", "DS1"),
    *mv("MOVEL", "'ZZ'", "DS1"), *sc("b", "DS1"),
    *mv("MOVE", "'7'", "SZ"), *sn("c", "SZ"),
    *sc("d", "DS1"),
    ev("ARR(1) = 'aaaa'"), ev("ARR(2) = 'bbbb'"), ev("ARR(3) = 'cccc'"),
    *mv("MOVE", "'X'", "ARR(I)"), *mv("MOVEL", "'Y'", "ARR(3)"),
    *show("'f ' + ARR(1) + ARR(2) + ARR(3)"),
    *mv("MOVE", "ARR(I)", "ZA(1)", "999"), *sn("g", "ZA(1)"),
    *mv("MOVE", "SZ", "ARR(1)"), *sc("h", "ARR(1)"),
])

# 16: MOVEA between arrays.
member("MOV016", [
    S("A2", 2, kw="DIM(4)"), S("A3", 3, kw="DIM(3)"), S("A5", 5, kw="DIM(2)"),
    S("I", 3, "P", "0"),
], [
    ev("A3(1) = 'abc'"), ev("A3(2) = 'def'"), ev("A3(3) = 'ghi'"),
    ev("A2 = '..'"),
    C(op="MOVEA", f2="A3", res="A2"),
    *show("'a ' + A2(1) + A2(2) + A2(3) + A2(4)"),
    ev("A5 = '-----'"),
    C(op="MOVEA", f2="A3", res="A5"),
    *show("'b ' + A5(1) + A5(2)"),
    ev("A2 = '..'"),
    C(op="MOVEA", f2="A3(2)", res="A2(2)"),
    *show("'c ' + A2(1) + A2(2) + A2(3) + A2(4)"),
    ev("A5 = '-----'"),
    C(op="MOVEA(P)", f2="A3(3)", res="A5"),
    *show("'d [' + A5(1) + A5(2) + ']'"),
    ev("A2 = '..'"), ev("I = 3"),
    C(op="MOVEA", f2="A5", res="A2(I)"),
    *show("'e ' + A2(1) + A2(2) + A2(3) + A2(4)"),
    ev("A3 = 'zzz'"),
    C(op="MOVEA(P)", f2="A2(4)", res="A3(2)"),
    *show("'f [' + A3(1) + A3(2) + A3(3) + ']'"),
])

# 17: MOVEA between fields and arrays.
member("MOV017", [
    S("A2", 2, kw="DIM(4)"), S("C5", 5), S("C10", 10), S("I", 3, "P", "0"),
], [
    ev("A2 = '..'"),
    C(op="MOVEA", f2="'abcde'", res="A2"),
    *show("'a ' + A2(1) + A2(2) + A2(3) + A2(4)"),
    ev("A2 = '..'"),
    C(op="MOVEA(P)", f2="'abc'", res="A2"),
    *show("'b [' + A2(1) + A2(2) + A2(3) + A2(4) + ']'"),
    ev("A2 = '..'"), ev("I = 2"),
    C(op="MOVEA", f2="'xyz'", res="A2(I)"),
    *show("'c ' + A2(1) + A2(2) + A2(3) + A2(4)"),
    ev("A2(1) = 'ab'"), ev("A2(2) = 'cd'"), ev("A2(3) = 'ef'"), ev("A2(4) = 'gh'"),
    ev("C5 = '*****'"),
    C(op="MOVEA", f2="A2", res="C5"),
    *sc("d", "C5"),
    ev("C10 = '**********'"),
    C(op="MOVEA", f2="A2(3)", res="C10"),
    *sc("e", "C10"),
    ev("C10 = '**********'"),
    C(op="MOVEA(P)", f2="A2(3)", res="C10"),
    *sc("f", "C10"),
    ev("C5 = 'vwxyz'"), ev("A2 = '..'"),
    C(op="MOVEA", f2="C5", res="A2(2)"),
    *show("'g ' + A2(1) + A2(2) + A2(3) + A2(4)"),
    ev("A2 = '..'"),
    C(op="MOVEA", f2="*ALL'q'", res="A2"),
    *show("'h ' + A2(1) + A2(2) + A2(3) + A2(4)"),
])

# 18: MOVEA of numeric arrays and indicators.
member("MOV018", [
    S("Z3", 3, "S", "0", "DIM(3)"), S("Z2", 3, "S", "0", "DIM(4)"),
    S("ZC", 2, kw="DIM(4)"), S("C6", 6),
], [
    C(op="SETOFF", hi="10", lo="15", eq="19"), C(op="SETOFF", hi="20", lo="21", eq="99"),
    ev("Z3(1) = 123"), ev("Z3(2) = 456"), ev("Z3(3) = -789"), ev("Z2 = 5"),
    C(op="MOVEA", f2="Z3", res="Z2(2)"),
    *show("'a ' + %EDITC(Z2(1):'X') + %EDITC(Z2(2):'X') + %EDITC(Z2(3):'X') + %EDITC(Z2(4):'X')"),
    *show("'b ' + %CHAR(Z2(1)) + ' ' + %CHAR(Z2(4))"),
    C(op="MOVEA", f2="'0110'", res="*IN(11)"),
    *show("'c ' + *IN10 + *IN11 + *IN12 + *IN13 + *IN14 + *IN15"),
    ev("C6 = '......'"),
    C(op="MOVEA", f2="*IN(11)", res="C6"),
    *sc("d", "C6"),
    C(op="MOVEA", f2="*ALL'1'", res="*IN(20)"),
    *show("'e ' + *IN19 + *IN20 + *IN21 + *IN99"),
    C(op="MOVEA(P)", f2="'1'", res="*IN(20)"),
    *show("'f ' + *IN20 + *IN21 + *IN99"),
])

# 19: MOVE with resulting indicators.
member("MOV019", [
    S("Z5", 5, "S", "0"), S("C5", 5),
], [
    C(op="MOVE", f2="'00000'", res="Z5", hi="31", lo="32", eq="33"),
    *show("'a ' + *IN31 + *IN32 + *IN33"),
    C(op="MOVE", f2="'0001J'", res="Z5", hi="31", lo="32", eq="33"),
    *show("'b ' + *IN31 + *IN32 + *IN33"),
    C(op="MOVE", f2="'00012'", res="Z5", hi="31", lo="32", eq="33"),
    *show("'c ' + *IN31 + *IN32 + *IN33"),
    C(op="MOVE", f2="*BLANKS", res="C5", eq="34"),
    *show("'d ' + *IN34"),
    C(op="MOVEL", f2="'a'", res="C5", eq="34"),
    *show("'e ' + *IN34"),
    C(op="MOVE", f2="-5", res="Z5", hi="31", lo="32", eq="33"),
    *show("'f ' + *IN31 + *IN32 + *IN33"),
])

# 20: the zone moves.
member("MOV020", [
    S("C2", 2, kw="INZ('AB')"), S("C3", 3), S("Z3", 3, "S", "0", "INZ(-123)"),
    S("Z4", 4, "S", "0"),
], [
    *mv("MHHZO", "'J9'", "C3", "'ABC'"), *sc("a", "C3"),
    *mv("MHLZO", "'J9'", "C3", "'ABC'"), *sc("b", "C3"),
    *mv("MLHZO", "'9J'", "C3", "'ABC'"), *sc("c", "C3"),
    *mv("MLLZO", "'AJ'", "C3", "'ABC'"), *sc("d", "C3"),
    *mv("MLLZO", "Z3", "C3", "'ABC'"), *sc("e", "C3"),
    *mv("MLLZO", "C2", "Z4", "1234"), *sn("f", "Z4"),
    *mv("MLHZO", "Z3", "C3", "'ABC'"), *sc("g", "C3"),
    *mv("MHLZO", "'J'", "Z4", "1234"), *sn("h", "Z4"),
])

# 21: long and short moves of decimals and signs.
member("MOV021", [
    S("Z9", 9, "S", "4", "INZ(-1234.5678)"), S("Z31", 31, "S", "0"),
    S("C40", 40), S("Z1", 1, "S", "0"), S("P1", 1, "P", "0"),
    S("C1", 1),
], [
    *mv("MOVE", "Z9", "Z31", "0"), *sn("a", "Z31"),
    *mv("MOVEL", "Z9", "Z31", "0"), *sn("b", "Z31"),
    *mv("MOVE(P)", "Z9", "C40"), *sc("c", "C40"),
    *mv("MOVE", "Z9", "Z1", "0"), *sn("d", "Z1"),
    *mv("MOVEL", "Z9", "Z1", "0"), *sn("e", "Z1"),
    *mv("MOVE", "Z9", "P1", "0"), *sn("f", "P1"),
    *mv("MOVE", "Z9", "C1", "'x'"), *sc("g", "C1"),
    *mv("MOVEL", "Z9", "C1", "'x'"), *sc("h", "C1"),
    *mv("MOVE", "'K'", "P1", "0"), *sn("i", "P1"),
])

# 22: odd character data into numbers.
member("MOV022", [
    S("Z4", 4, "S", "0"), S("P4", 4, "P", "0"),
], [
    *mv("MOVE", "'abcd'", "Z4", "0"), *sn("b", "Z4"),
    *mv("MOVE", "'{}AJ'", "Z4", "0"), *sn("c", "Z4"),
    *mv("MOVE", "'12 }'", "Z4", "0"), *sn("d", "Z4"),
    *mv("MOVE", "'STUV'", "Z4", "0"), *sn("e", "Z4"),
    *mv("MOVE", "'12 }'", "P4", "0"), *sn("g", "P4"),
    *mv("MOVE", "'STUV'", "P4", "0"), *sn("h", "P4"),
    *mv("MOVEL", "'9'", "P4", "-1234"), *sn("i", "P4"),
])


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, lines in M.items():
        text = "\n".join(lines) + "\n"
        for i, l in enumerate(text.splitlines(), 1):
            if len(l) > 80:
                raise SystemExit(f"{name} line {i} is {len(l)} wide")
        with open(os.path.join(OUT, name + ".rpgle"), "w", newline="\n") as f:
            f.write(text)
    print(f"{len(M)} members")


if __name__ == "__main__":
    main()
