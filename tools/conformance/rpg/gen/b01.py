"""Corpus batch 1: the specification forms. Writes ../b01spec/*.rpgle.

usage: python -I b01.py
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fixed import C, CX, D, F, H, O_fld, O_rec, P, PRT_D, PRT_F, comment, free, place, show  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b01spec")
MEMBERS = {}


def member(name, lines):
    MEMBERS[name] = lines


# Every D-spec data type in length and position notation.
member("SPEC001", [
    H("DFTACTGRP(*NO)"),
    PRT_F,
    PRT_D,
    D("PK", "S", to="7", typ="P", dec="2", kw="INZ(-12.5)"),
    D("ZN", "S", to="5", typ="S", dec="0", kw="INZ(42)"),
    D("BN", "S", to="9", typ="B", dec="0", kw="INZ(123456)"),
    D("IN", "S", to="10", typ="I", dec="0", kw="INZ(-7)"),
    D("UN", "S", to="5", typ="U", dec="0", kw="INZ(65535)"),
    D("CH", "S", to="10", kw="INZ('abc')"),
    D("VC", "S", to="20", typ="A", kw="VARYING INZ('vary')"),
    D("NUM", "S", to="5", dec="1", kw="INZ(3.5)"),
    D("REC", "DS"),
    D(" ID", frm="1", to="5", typ="S", dec="0"),
    D(" NM", frm="6", to="15"),
    D(" AMT", frm="16", to="19", typ="P", dec="2"),
    CX("EVAL", "ID = 12"),
    CX("EVAL", "NM = 'Name'"),
    CX("EVAL", "AMT = 99.99"),
] + show("'PK=' + %CHAR(PK) + ' ZN=' + %CHAR(ZN) + ' BN=' + %CHAR(BN)")
  + show("'IN=' + %CHAR(IN) + ' UN=' + %CHAR(UN) + ' NUM=' + %CHAR(NUM)")
  + show("'CH=' + CH + '|' + ' VC=' + VC + '|'")
  + show("'REC=' + REC")
  + show("'ID=' + %CHAR(ID) + ' AMT=' + %CHAR(AMT)")
  + [CX("EVAL", "*INLR = *ON")])

# Fixed-form arithmetic with resulting indicators.
member("SPEC002", [
    PRT_F,
    PRT_D,
    D("A", "S", to="5", dec="0", kw="INZ(10)"),
    D("B", "S", to="5", dec="0", kw="INZ(3)"),
    D("R", "S", to="7", dec="2"),
    D("S", "S", to="3", dec="0"),
    C("A", "ADD", "B", "R", hi="10", lo="11", eq="12"),
] + show("'ADD ' + %CHAR(R) + ' ' + *IN10 + *IN11 + *IN12") + [
    C("A", "SUB", "B", "R", hi="10", lo="11", eq="12"),
] + show("'SUB ' + %CHAR(R) + ' ' + *IN10 + *IN11 + *IN12") + [
    C("B", "SUB", "A", "R", hi="10", lo="11", eq="12"),
] + show("'SUB2 ' + %CHAR(R) + ' ' + *IN10 + *IN11 + *IN12") + [
    C("A", "MULT", "B", "R"),
] + show("'MULT ' + %CHAR(R)") + [
    C("A", "DIV", "B", "R"),
] + show("'DIV ' + %CHAR(R)") + [
    C(op="Z-ADD", f2="123456", res="S"),
] + show("'ZADD ' + %CHAR(S)") + [
    C(op="Z-SUB", f2="5", res="R"),
] + show("'ZSUB ' + %CHAR(R)") + [
    C(op="ADD", f2="1", res="R"),
] + show("'ADD1 ' + %CHAR(R)") + [
    C(op="Z-ADD(H)", f2="2.555", res="R"),
] + show("'ZADDH ' + %CHAR(R)") + [
    C("A", "ADD", "0", "S", eq="13"),
] + show("'IND13 ' + *IN13") + [CX("EVAL", "*INLR = *ON")])

# Extended factor 2 over several lines; literals continued with + and -.
member("SPEC003", [
    PRT_F,
    PRT_D,
    D("TOTAL", "S", to="9", dec="2"),
    D("TXT", "S", to="40", typ="A", kw="VARYING"),
    CX("EVAL", "TOTAL = 1 +"),
    CX("", "2 +"),
    CX("", "3.25"),
    CX("EVAL", "TXT = 'first part, +"),
    CX("", "      second part'"),
] + show("'T=' + %CHAR(TOTAL) + ' [' + TXT + ']'") + [
    CX("EVAL", "TXT = 'keep-"),
    CX("", "  blanks'"),
] + show("'[' + TXT + ']'") + [CX("EVAL", "*INLR = *ON")])

# Keywords continued over D lines; long names with ...
member("SPEC004", [
    PRT_F,
    PRT_D,
    D("SHORT", "S", to="10", kw="INZ('x')"),
    D("", kw="VARYING"),
    place([(6, "D"), (7, "A_VERY_LONG_FIELD_NAME_INDEED...")]),
    D("", "S", to="5", dec="0", kw="INZ(17)"),
    D("ARR", "S", to="3", dec="0", kw="DIM(4)"),
    D("", kw="INZ(5)"),
] + show("'[' + SHORT + '] ' + %CHAR(A_VERY_LONG_FIELD_NAME_INDEED)")
  + show("%CHAR(ARR(1)) + ' ' + %CHAR(ARR(4)) + ' ' + %CHAR(%ELEM(ARR))")
  + [CX("EVAL", "*INLR = *ON")])

# Conditioning indicators with AN and OR lines.
member("SPEC005", [
    PRT_F,
    PRT_D,
    C(op="SETON", hi="01", lo="02"),
    C(op="SETOFF", hi="03"),
    C(cond=" 01"),
    CX("EVAL", "LINE = 'both 01 and 02'", cond=" 02", level="AN"),
    C(cond=" 01"),
    C(op="WRITE", f2="QSYSPRT", res="LINE", cond=" 02", level="AN"),
    CX("EVAL", "LINE = 'not 03'", cond="N03"),
    C(op="WRITE", f2="QSYSPRT", res="LINE", cond="N03"),
    C(cond=" 03"),
    CX("EVAL", "LINE = '03 or 02'", cond=" 02", level="OR"),
    C(cond=" 03"),
    C(op="WRITE", f2="QSYSPRT", res="LINE", cond=" 02", level="OR"),
    CX("EVAL", "LINE = 'never'", cond=" 03"),
    C(op="WRITE", f2="QSYSPRT", res="LINE", cond=" 03"),
    CX("EVAL", "*INLR = *ON"),
])

# IFxx/ANDxx/ORxx, DOWxx, DOUxx, CABxx with TAG and GOTO.
member("SPEC006", [
    PRT_F,
    PRT_D,
    D("I", "S", to="3", dec="0"),
    D("J", "S", to="3", dec="0", kw="INZ(5)"),
    C("J", "IFGT", "3"),
    C("J", "ANDLT", "9"),
] + show("'J between'") + [
    C(op="ENDIF"),
    C("J", "IFEQ", "1"),
    C("J", "OREQ", "5"),
] + show("'J is 1 or 5'") + [
    C(op="ELSE"),
] + show("'J is other'") + [
    C(op="ENDIF"),
    C(op="Z-ADD", f2="0", res="I"),
    C("I", "DOWLT", "3"),
    C(op="ADD", f2="1", res="I"),
] + show("'DOW ' + %CHAR(I)") + [
    C(op="ENDDO"),
    C("I", "DOUGE", "5"),
    C(op="ADD", f2="1", res="I"),
] + show("'DOU ' + %CHAR(I)") + [
    C(op="ENDDO"),
    C("I", "CABEQ", "5", "SKIP"),
] + show("'not skipped'") + [
    C("SKIP", "TAG"),
] + show("'after tag'") + [
    C(op="GOTO", f2="DONE"),
] + show("'not reached'") + [
    C("DONE", "TAG"),
    CX("EVAL", "*INLR = *ON"),
])

# Mixed source: free-form lines between fixed specs.
member("SPEC008", [
    PRT_F,
    PRT_D,
    D("COUNT", "S", to="10", typ="I", dec="0"),
    free("dcl-s name char(10) inz('mixed');"),
    CX("EVAL", "COUNT = 2"),
    free("count += 3;"),
    free("line = 'count=' + %char(count) + ' name=' + %trim(name);"),
    C(op="WRITE", f2="QSYSPRT", res="LINE"),
    free("if count = 5;"),
    free("  line = 'five';"),
    free("  write QSYSPRT line;"),
    free("endif;"),
    free("*inlr = *on;"),
])

# A /FREE block.
member("SPEC009", [
    PRT_F,
    PRT_D,
    D("X", "S", to="5", dec="0"),
    place([(7, "/FREE")]),
    free("x = 7 * 6;"),
    free("line = 'x=' + %char(x);"),
    free("write QSYSPRT line;"),
    free("*inlr = *on;"),
    place([(7, "/END-FREE")]),
])

# Comments of every kind.
member("SPEC010", [
    comment("A comment line"),
    PRT_F,
    PRT_D,
    "",
    place([(6, "D"), (7, "V"), (24, "S"), (39, "5"), (42, "0"), (81, "comment area")]),
    free("// a free-form comment line"),
    CX("EVAL", "V = 9"),
    place([(6, "C"), (26, "EVAL"), (36, "LINE = 'v=' + %CHAR(V)"), (81, "trailing words")]),
    C(op="WRITE", f2="QSYSPRT", res="LINE"),
    comment("last comment"),
    CX("EVAL", "*INLR = *ON"),
])

# Compile-time arrays.
member("SPEC011", [
    PRT_F,
    PRT_D,
    D("MONTHS", "S", to="3", kw="DIM(12) CTDATA PERRCD(6)"),
    D("CODES", "S", to="2", dec="0", kw="DIM(3) CTDATA PERRCD(3)"),
    D("I", "S", to="3", dec="0"),
    C(op="Z-ADD", f2="1", res="I"),
    C("I", "DOWLE", "12"),
] + show("%CHAR(I) + '=' + MONTHS(I)") + [
    C(op="ADD", f2="1", res="I"),
    C(op="ENDDO"),
] + show("%CHAR(CODES(1)) + ' ' + %CHAR(CODES(2)) + ' ' + %CHAR(CODES(3))") + [
    CX("EVAL", "*INLR = *ON"),
    "**CTDATA MONTHS",
    "JANFEBMARAPRMAYJUN",
    "JULAUGSEPOCTNOVDEC",
    "**CTDATA CODES",
    "010203",
])

# Conditional compilation.
member("SPEC013", [
    PRT_F,
    PRT_D,
    place([(7, "/DEFINE ONE")]),
    place([(7, "/IF DEFINED(ONE)")]),
] + show("'one defined'") + [
    place([(7, "/ELSE")]),
] + show("'one not defined'") + [
    place([(7, "/ENDIF")]),
    place([(7, "/UNDEFINE ONE")]),
    place([(7, "/IF NOT DEFINED(ONE)")]),
] + show("'one gone'") + [
    place([(7, "/ENDIF")]),
    place([(7, "/IF DEFINED(*V7R3M0)")]),
] + show("'at least 7.3'") + [
    place([(7, "/ENDIF")]),
    place([(7, "/IF DEFINED(*V9R9M0)")]),
] + show("'never'") + [
    place([(7, "/ELSEIF DEFINED(*CRTBNDRPG)")]),
] + show("'crtbndrpg'") + [
    place([(7, "/ENDIF")]),
    CX("EVAL", "*INLR = *ON"),
])

# /EOF ends the member.
member("SPEC014", [
    PRT_F,
    PRT_D,
] + show("'before eof'") + [
    CX("EVAL", "*INLR = *ON"),
    place([(7, "/EOF")]),
    "this is not RPG at all",
    "     C                   EVAL      LINE = 'never'",
])

# Procedures in fixed form.
member("SPEC016", [
    H("DFTACTGRP(*NO)"),
    PRT_F,
    PRT_D,
    D("TWICE", "PR", to="9", dec="2", typ="P"),
    D("", to="7", dec="2", typ="P", kw="VALUE"),
] + show("'twice=' + %CHAR(TWICE(4.25))") + [
    CX("EVAL", "*INLR = *ON"),
    P("TWICE", "B"),
    D("", "PI", to="9", dec="2", typ="P"),
    D("N", to="7", dec="2", typ="P", kw="VALUE"),
    CX("RETURN", "N * 2"),
    P("TWICE", "E"),
])

# Lower-case specification letters and opcodes.
member("SPEC018", [
    place([(6, "f"), (7, "qsysprt"), (17, "o"), (22, "f"), (24, "132"), (36, "printer")]),
    place([(6, "d"), (7, "line"), (24, "ds"), (37, "132")]),
    place([(6, "d"), (7, "n"), (24, "s"), (37, "5"), (42, "0"), (44, "inz(31)")]),
    place([(6, "c"), (26, "eval"), (36, "line = 'lower ' + %char(n)")]),
    place([(6, "c"), (26, "write"), (36, "qsysprt"), (50, "line")]),
    place([(6, "c"), (26, "eval"), (36, "*inlr = *on")]),
])

# Free-form statements over several lines, several on a line.
member("SPEC019", [
    "**FREE",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
    "dcl-s a int(10) inz(1); dcl-s b int(10) inz(2);",
    "dcl-s c",
    "      packed(9:2)",
    "      inz(10.5);",
    "c = c",
    "    + a",
    "    * b;",
    "line = 'c=' +",
    "       %char(c); write QSYSPRT line;",
    "if a < b and",
    "   c > 0;",
    "  line = 'yes'; write QSYSPRT line;",
    "endif;",
    "*inlr = *on;",
])

# Hex literals, figuratives, *ALL.
member("SPEC020", [
    "**FREE",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
    "dcl-s c char(6);",
    "dcl-s n packed(5:0);",
    "c = x'C1C2C3';",
    "line = '[' + c + ']'; write QSYSPRT line;",
    "c = *all'xy';",
    "line = '[' + c + ']'; write QSYSPRT line;",
    "c = *blanks;",
    "if c = *blanks;",
    "  line = 'blank'; write QSYSPRT line;",
    "endif;",
    "n = *hival;",
    "line = %char(n); write QSYSPRT line;",
    "n = *loval;",
    "line = %char(n); write QSYSPRT line;",
    "n = *zeros;",
    "if n = *zero;",
    "  line = 'zero'; write QSYSPRT line;",
    "endif;",
    "*inlr = *on;",
])

# Program-described output with O specs and EXCEPT.
member("SPEC017", [
    PRT_F.replace("PRINTER", "PRINTER OFLIND(*INOF)"),
    D("AMOUNT", "S", to="7", dec="2", kw="INZ(1234.5)"),
    D("NAME", "S", to="10", kw="INZ('Widget')"),
    C(op="EXCEPT", f2="HDR"),
    C(op="EXCEPT", f2="DTL"),
    CX("EVAL", "AMOUNT = -42.75"),
    C(op="EXCEPT", f2="DTL"),
    CX("EVAL", "*INLR = *ON"),
    O_rec("QSYSPRT", "E", exc="HDR", sa="2"),
    O_fld(const="'Item'", end="10"),
    O_fld(const="'Amount'", end="30"),
    O_rec("", "E", exc="DTL", sa="1"),
    O_fld("NAME", end="12"),
    O_fld("AMOUNT", end="30", edit="J"),
    O_fld("AMOUNT", end="45", edit="1"),
])

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import b01more  # noqa: E402

b01more.add(member)
INCS = b01more.INCS

for name, lines in MEMBERS.items():
    with open(os.path.join(OUT, name + ".rpgle"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
for name, lines in INCS.items():
    with open(os.path.join(OUT, name + ".rpginc"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
print(len(MEMBERS), "members")
