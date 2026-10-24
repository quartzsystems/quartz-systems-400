"""Corpus batch 13: listing fidelity. Each member exercises a part of the
compile listing: /COPY members (*SHOWCPY, *NOSHOWCPY, nested,
/INCLUDE), conditional compilation (*SHOWSKP, *NOSHOWSKP), the
OPTION values (*SRCSTMT, *NOXREF, *NOGEN, *NOEXPDDS, *NOEXT,
*NOUNREF), INDENT, the command's options on the first page, long
sources over several pages, compile-time data, fixed and free mixes,
and every section (the cross-reference with each type's attributes,
external references, the message summary, the final summary). Writes
../b13lst/*.rpgle and the members they bring (*.rpginc). Most only
compile; the listing is what the kit compares.

usage: python -I b13.py
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fixed import C, CX, D, F, H, I_rec, O_fld, O_rec, P, PRT_D, PRT_F, comment, free, place, show  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b13lst")
CMD = "//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT)"
HEAD = ["**FREE", "ctl-opt dftactgrp(*no);", "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"]
M = {}
INC = {}


def cmd(extra):
    return CMD + " " + extra


def p(expr):
    return [f"line = {expr};", "write QSYSPRT line;"]


END = ["*inlr = *on;"]

# /COPY: a free-form member in free-form source, shown (*SHOWCPY).
INC["LSTC1"] = ["**FREE", "// Prototypes for the listing batch.", "dcl-pr twice int(10);", "  n int(10) value;", "end-pr;"]
M["LST001"] = ["**FREE", "//; with: LSTC1", "//; run: no"] + HEAD[1:] + [
    "/copy QRPGLESRC,LSTC1",
    "dcl-s r int(10);",
    "r = twice(21);",
] + p("%char(r)") + END + [
    "dcl-proc twice;",
    "  dcl-pi *n int(10);",
    "    n int(10) value;",
    "  end-pi;",
    "  return n * 2;",
    "end-proc;",
]

# The same with OPTION(*NOSHOWCPY).
M["LST002"] = ["**FREE", "//; with: LSTC1", "//; run: no", cmd("OPTION(*NOSHOWCPY)")] + M["LST001"][3:]

# Nested copies: a fixed-form member that copies a free-form one.
INC["LSTC2"] = [
    comment("A fixed-form member with a nested copy."),
    D("LIMIT", "C", kw="CONST(99)"),
    "      /COPY QRPGLESRC,LSTC3",
    D("COUNT", "S", to="5", typ="P", dec="0", kw="INZ(1)"),
]
INC["LSTC3"] = ["**FREE", "dcl-s label char(8) inz('nested');"]
M["LST003"] = ["**FREE", "//; with: LSTC2 LSTC3", "//; run: no"] + HEAD[1:] + [
    "/copy LSTC2",
] + p("label + %char(count) + %char(limit)") + END

# /INCLUDE and a copy of a missing member.
M["LST004"] = ["**FREE", "//; with: LSTC3", "//; run: no"] + HEAD[1:] + [
    "/include QRPGLESRC,LSTC3",
    "/copy QRPGLESRC,NOSUCH",
] + p("label") + END

# Conditional compilation, the default *NOSHOWSKP.
COND = [
    "/define ONE",
    "/if defined(ONE)",
    "dcl-s a char(5) inz('one');",
    "/else",
    "dcl-s a char(5) inz('none');",
    "dcl-s b char(5);",
    "/endif",
    "/if not defined(TWO)",
    "dcl-s c int(10);",
    "/elseif defined(ONE)",
    "dcl-s c int(5);",
    "/else",
    "dcl-s c int(3);",
    "/endif",
    "/undefine ONE",
    "/if defined(ONE)",
    "c = 1;",
    "c = 2;",
    "c = 3;",
    "/endif",
]
M["LST005"] = ["**FREE", "//; run: no"] + HEAD[1:] + COND + p("a + %char(c)") + END
M["LST006"] = ["**FREE", "//; run: no", cmd("OPTION(*SHOWSKP)")] + HEAD[1:] + COND + p("a + %char(c)") + END

# Nested conditions in fixed form, *SHOWSKP, with DEFINE on the command.
M["LST007"] = [
    comment("Nested conditions; DEFINE names some."),
    "      *; run: no",
    "      *; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT) OPTION(*SHOWSKP) DEFINE(ALPHA)",
    PRT_F,
    PRT_D,
    "      /IF DEFINED(ALPHA)",
    "      /IF DEFINED(BETA)",
    D("X", "S", to="10", kw="INZ('ab')"),
    "      /ELSE",
    D("X", "S", to="10", kw="INZ('a')"),
    "      /ENDIF",
    "      /ELSE",
    D("X", "S", to="10", kw="INZ('none')"),
    "      /ENDIF",
] + show("X") + [C(op="SETON", hi="LR")]

# OPTION(*SRCSTMT): statement numbers from the sequence numbers.
M["LST008"] = ["**FREE", "//; run: no", cmd("OPTION(*SRCSTMT)")] + HEAD[1:] + [
    "dcl-s n int(10);",
    "n = 'x';",
    "for n = 1 to 3;",
] + ["  " + x for x in p("%char(n)")] + ["endfor;"] + END

# OPTION(*NOXREF), *NOGEN, *NOEXT, *NOUNREF.
BODY = [
    "dcl-s used int(10) inz(1);",
    "dcl-s unused char(3);",
    "dcl-s spare packed(5:2);",
    "dcl-pr ext1 extproc('LSTEXT1');",
    "end-pr;",
] + p("%char(used)") + END
M["LST009"] = ["**FREE", "//; run: no", cmd("OPTION(*NOXREF)")] + HEAD[1:] + BODY
M["LST010"] = ["**FREE", "//; run: no", cmd("OPTION(*NOGEN)")] + HEAD[1:] + BODY
M["LST011"] = ["**FREE", "//; run: no", cmd("OPTION(*NOEXT)")] + HEAD[1:] + BODY
M["LST012"] = ["**FREE", "//; run: no", cmd("OPTION(*NOUNREF)")] + HEAD[1:] + BODY
M["LST013"] = ["**FREE", "//; run: no", cmd("OPTION(*NOXREF *NOGEN *NOSECLVL *SHOWCPY *NOEXPDDS *NOEXT *NOSHOWSKP *SRCSTMT *NODEBUGIO *NOUNREF)")] + HEAD[1:] + BODY

# Externally described files with and without *EXPDDS.
EXTF = [
    "//; dds: CUSTF",
    "//; sqlfile: CUSTF",
    "ctl-opt dftactgrp(*no);",
    "dcl-f CUSTF keyed usage(*update:*output);",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
    "dcl-ds keys extname('CUSTF':*KEY) qualified end-ds;",
    "keys.CUSNO = 101;",
    "chain %kds(keys) CUSTF;",
    "if %found(CUSTF);",
    "  BAL += 1;",
    "  update CUSTR;",
    "endif;",
] + p("NAME + %char(BAL)") + END
M["LST014"] = ["**FREE"] + EXTF
M["LST015"] = ["**FREE", cmd("OPTION(*NOEXPDDS)")] + EXTF

# INDENT on fixed-form structured operations, and on free form.
FIXED_LOOP = [
    PRT_F,
    PRT_D,
    D("I", "S", to="5", typ="P", dec="0"),
    D("J", "S", to="5", typ="P", dec="0"),
    C(f1="1", op="DO", f2="3", res="I"),
    C(f1="I", op="IFEQ", f2="2"),
    CX("EVAL", "J = J + 10"),
    C(op="ELSE"),
    CX("EVAL", "J = J + 1"),
    C(op="ENDIF"),
    C(op="ENDDO"),
] + show("%CHAR(J)") + [C(op="SETON", hi="LR")]
M["LST016"] = [comment("INDENT with two characters."), "      *; run: no",
               "      *; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT) INDENT('| ')"] + FIXED_LOOP
M["LST017"] = ["**FREE", "//; run: no", cmd("INDENT('>')")] + HEAD[1:] + [
    "dcl-s i int(10);",
    "dcl-s j int(10);",
    "for i = 1 to 3;",
    "  if i = 2;",
    "    j += 10;",
    "  else;",
    "    j += 1;",
    "  endif;",
    "endfor;",
] + p("%char(j)") + END

# A long source: several pages, a diagnostic near each page end.
LONG = ["**FREE", "//; run: no"] + HEAD[1:]
for k in range(1, 61):
    LONG.append(f"dcl-s v{k:02} packed(7:2) inz({k});")
for k in range(1, 61):
    LONG += [f"v{k:02} = v{k:02} * 2;"]
    if k % 15 == 0:
        LONG.append(f"eval(h) v{k:02} = v{k:02} + 1;")
LONG += p("%char(v01)") + END
M["LST018"] = LONG

# Compile-time data: arrays, a table with its alternate, ** forms.
M["LST019"] = [
    comment("Compile-time arrays and tables."),
    "      *; run: no",
    PRT_F,
    PRT_D,
    D("ARR", "S", to="4", kw="DIM(3) CTDATA PERRCD(1)"),
    D("TABX", "S", to="3", kw="DIM(2) CTDATA PERRCD(1)"),
    D("TABY", "S", to="5", kw="DIM(2) ALT(TABX)"),
    D("NUMS", "S", to="3", typ="S", dec="0", kw="DIM(4) CTDATA PERRCD(2)"),
] + show("ARR(1) + ARR(3) + TABX + TABY") + [C(op="SETON", hi="LR"),
    "**CTDATA ARR",
    "aaaa",
    "bbbb",
    "cccc",
    "**CTDATA TABX",
    "AAAone",
    "BBBtwo",
    "**CTDATA NUMS",
    "001002",
    "003004",
]
M["LST020"] = [
    comment("The old ** separators, in definition order."),
    "      *; run: no",
    PRT_F,
    PRT_D,
    D("A1", "S", to="2", kw="DIM(2) CTDATA PERRCD(2)"),
    D("A2", "S", to="3", kw="DIM(1) CTDATA"),
] + show("A1(1) + A1(2) + A2(1)") + [C(op="SETON", hi="LR"),
    "**",
    "xxyy",
    "**",
    "zzz",
]

# Fixed and free mixed: fixed definitions, a /free block, calculations
# in columns 8-80, a fixed calculation after.
M["LST021"] = [
    comment("Fixed and free in one member."),
    "      *; run: no",
    H("DFTACTGRP(*NO) DATFMT(*ISO)"),
    PRT_F,
    PRT_D,
    D("N", "S", to="7", typ="P", dec="2", kw="INZ(1.5)"),
    D("NAME", "S", to="10", kw="INZ('mix')"),
    free("dcl-s extra int(10) inz(4);"),
    "      /free",
    "        n = n * 2;",
    "        if n > 2;",
    "          name = 'big';",
    "        endif;",
    "      /end-free",
    free("n += extra;"),
    C(f1="N", op="ADD", f2="1", res="N"),
] + show("%TRIM(NAME) + ' ' + %CHAR(N)") + [C(op="SETON", hi="LR")]

# Cross-reference attributes of every type.
M["LST022"] = ["**FREE", "//; run: no"] + HEAD[1:] + [
    "dcl-s c1 char(10);",
    "dcl-s c2 varchar(20);",
    "dcl-s c3 varchar(10:4);",
    "dcl-s u1 ucs2(5);",
    "dcl-s u2 varucs2(6);",
    "dcl-s g1 graph(4);",
    "dcl-s p1 packed(7:2);",
    "dcl-s z1 zoned(9:3);",
    "dcl-s b1 bindec(4:1);",
    "dcl-s i1 int(3);",
    "dcl-s i2 int(5);",
    "dcl-s i3 int(10);",
    "dcl-s i4 int(20);",
    "dcl-s n1 uns(10);",
    "dcl-s f1 float(4);",
    "dcl-s f2 float(8);",
    "dcl-s d1 date;",
    "dcl-s d2 date(*mdy);",
    "dcl-s d3 date(*jul-);",
    "dcl-s t1 time;",
    "dcl-s t2 time(*hms.);",
    "dcl-s s1 timestamp;",
    "dcl-s s2 timestamp(3);",
    "dcl-s ptr pointer;",
    "dcl-s pp pointer(*proc);",
    "dcl-s ok ind;",
    "dcl-c k1 'text';",
    "dcl-c k2 42;",
] + p("c1 + c2 + c3 + %char(u1) + %char(u2) + %char(p1) + %char(z1) + %char(b1)") + p(
    "%char(i1) + %char(i2) + %char(i3) + %char(i4) + %char(n1) + %char(f1) + %char(f2)") + p(
    "%char(d1) + %char(d2) + %char(d3) + %char(t1) + %char(t2) + %char(s1) + %char(s2)") + [
    "ptr = %addr(c1);",
    "ok = (ptr <> *null) and (pp = *null);",
] + p("ok + k1 + %char(k2)") + END

# Arrays, data structures, qualified and LIKEDS, based, overlay.
M["LST023"] = ["**FREE", "//; run: no"] + HEAD[1:] + [
    "dcl-s arr char(3) dim(5);",
    "dcl-s tab char(2) dim(3) ctdata;",
    "dcl-ds plain;",
    "  f1 char(4);",
    "  f2 packed(5:0);",
    "  whole char(7) overlay(plain);",
    "end-ds;",
    "dcl-ds tmpl qualified template;",
    "  id int(10);",
    "  nm char(8);",
    "end-ds;",
    "dcl-ds rec likeds(tmpl) dim(2);",
    "dcl-s buf char(20);",
    "dcl-s p pointer inz(%addr(buf));",
    "dcl-s view char(5) based(p);",
    "arr(2) = 'b';",
    "f1 = 'abcd';",
    "rec(1).id = 7;",
    "rec(2).nm = 'two';",
] + p("arr(2) + whole + %char(rec(1).id) + rec(2).nm + view + tab(1)") + END + ["**CTDATA tab", "t1", "t2", "t3"]

# Indicators, subroutines, tags, key and parameter lists, *IN.
M["LST024"] = [
    comment("Indicators and labels in the cross-reference."),
    "      *; run: no",
    PRT_F,
    PRT_D,
    D("A", "S", to="5", typ="P", dec="0", kw="INZ(3)"),
    D("B", "S", to="5", typ="P", dec="0", kw="INZ(4)"),
    C(f1="A", op="COMP", f2="B", hi="10", lo="20", eq="30"),
    C(op="EXSR", f2="SHOW", cond=" 20"),
    C(op="GOTO", f2="DONE", cond="N10"),
    C(op="SETON", hi="40"),
    C(f1="DONE", op="TAG"),
    CX("EVAL", "*IN(41) = *IN20"),
    C(op="SETON", hi="LR"),
    C(f1="SHOW", op="BEGSR"),
] + show("'a<b ' + *IN20 + *IN41") + [
    C(op="ENDSR"),
]

# Procedures with locals and parameters of each passing kind.
M["LST025"] = ["**FREE", "//; run: no"] + HEAD[1:] + [
    "dcl-s total packed(9:2);",
    "total = addup(1.5 : 2 : 'x');",
] + p("%char(total)") + END + [
    "dcl-proc addup;",
    "  dcl-pi *n packed(9:2);",
    "    a packed(7:2) value;",
    "    b int(10) const;",
    "    c char(1);",
    "  end-pi;",
    "  dcl-s work packed(9:2) static;",
    "  dcl-s unusedl char(2);",
    "  work = a + b;",
    "  c = 'y';",
    "  return work;",
    "end-proc;",
]

# A module: exported and imported fields, a bound procedure.
M["LST026"] = ["**FREE", "//; run: no",
               "//; cmd: CRTRPGMOD MODULE({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT)",
               "ctl-opt nomain;",
               "dcl-s shared int(10) export inz(5);",
               "dcl-s outside int(10) import;",
               "dcl-pr helper int(10) extproc('LSTHELP');",
               "  n int(10) value;",
               "end-pr;",
               "dcl-proc bump export;",
               "  dcl-pi *n int(10);",
               "  end-pi;",
               "  return helper(shared + outside);",
               "end-proc;",
               ]

# Many messages: severities 00, 10 and 20, wrapped texts, a summary.
M["LST027"] = ["**FREE", "//; run: no"] + HEAD[1:] + [
    "dcl-s a packed(5:2);",
    "dcl-s b packed(3:1) inz(12.34);",
    "dcl-s r packed(9:4);",
    "dcl-s d date;",
    "eval(h) r = a * 2;",
    "a = b + 1;",
    "d = %date('2026-13-01' : *iso);",
] + p("%char(r) + %char(a)") + END

# GENLVL(20): a severity-20 error and still created.
M["LST028"] = ["**FREE", "//; run: no", cmd("GENLVL(20)")] + HEAD[1:] + [
    "dcl-s a char(3) inz('abcdef');",
] + p("a") + END

# The command's options on the first page.
M["LST029"] = ["**FREE", "//; run: no", "//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DFTACTGRP(*NO) ACTGRP(LSTAG) DBGVIEW(*SOURCE) OUTPUT(*PRINT) TEXT('Listing options') TRUNCNBR(*NO)"] + [
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
] + p("'opts'") + END
M["LST030"] = ["**FREE", "//; run: no", "//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT) DEFINE(ONE TWO) OPTIMIZE(*BASIC) ALWNULL(*USRCTL)"] + HEAD[1:] + [
    "/if defined(TWO)",
    "dcl-s two char(3) inz('two');",
    "/endif",
] + p("two") + END

# Program-described output: buffer positions, edit codes and words.
M["LST031"] = [
    comment("Output buffer positions."),
    "      *; run: no",
    F("QSYSPRT", "O", fmt="F", rlen="132", dev="PRINTER", kw="OFLIND(*INOF)"),
    D("AMT", "S", to="9", typ="P", dec="2", kw="INZ(-1234.5)"),
    D("CNT", "S", to="5", typ="P", dec="0", kw="INZ(42)"),
    D("NAME", "S", to="8", kw="INZ('report')"),
    C(op="EXCEPT", f2="DTL"),
    C(op="SETON", hi="LR"),
    O_rec("QSYSPRT", "H", cond=" 1P", sb="", sa="2", kb="1"),
    O_fld(const="'Title'", end="10"),
    O_fld(field="PAGE", edit="Z", end="+3"),
    O_rec("QSYSPRT", "E", exc="DTL", sa="1"),
    O_fld(field="NAME", end="10"),
    O_fld(field="AMT", edit="J", end="+2"),
    O_fld(field="CNT", edit="Z", end="40"),
    O_fld(field="AMT", end="60", const="'  ,   , 0 .  -'"),
    O_fld(field="UDATE", edit="Y", end="75"),
]

# A cycle with control levels and match fields.
M["LST032"] = [
    "      *; dds: SALESF",
    "      *; sqlfile: SALESF",
    "      *; run: no",
    comment("Control levels L1 and L2 on a program-described file."),
    F("SALESF", "I", desig="P", fmt="F", rlen="22", dev="DISK"),
    PRT_F,
    D("TOT", "S", to="9", typ="P", dec="2"),
    I_rec("SALESF", "NS", rid="01"),
    place([(6, "I"), (37, "    1"), (42, "    2"), (49, "REGION"), (63, "L2")]),
    place([(6, "I"), (37, "    3"), (42, "    6"), (49, "REP"), (63, "L1")]),
    place([(6, "I"), (36, "P"), (37, "   17"), (42, "   20"), (47, " 2"), (49, "AMT")]),
    C(op="ADD", f2="AMT", res="TOT", cond="01"),
    C(op="Z-ADD", f2="0", res="TOT", level="L1"),
    O_rec("QSYSPRT", "T", cond=" L1", sa="1"),
    O_fld(field="REGION", end="4"),
    O_fld(field="REP", end="10"),
    O_fld(field="TOT", edit="J", end="25", blank="B"),
]

# An externally described data structure in fixed form (=D lines).
M["LST033"] = [
    "      *; dds: CUSTF",
    "      *; sqlfile: CUSTF",
    "      *; run: no",
    PRT_F,
    PRT_D,
    D("CUST", "DS", ext="E", kw="EXTNAME(CUSTF)"),
    D("KEYS", "DS", ext="E", kw="EXTNAME(CUSTF:*KEY) QUALIFIED PREFIX(K_)"),
] + show("%CHAR(CUSNO) + NAME + %CHAR(KEYS.K_CUSNO)") + [C(op="SETON", hi="LR")]

# Long names and continued names.
M["LST034"] = [
    comment("Names of more than fifteen characters."),
    "      *; run: no",
    PRT_F,
    PRT_D,
    D("A_REALLY_LONG_NAME_FOR_A_FIELD..."),
    D("", "S", to="5", typ="P", dec="0", kw="INZ(1)"),
    D("ANOTHER_VERY_LONG_NAME_HERE...."[:-1]),
    D("", "S", to="10", kw="INZ('x')"),
] + show("%CHAR(A_REALLY_LONG_NAME_FOR_A_FIELD) + ANOTHER_VERY_LONG_NAME_HERE") + [
    free("A_REALLY_LONG_NAME_FOR_A_FIELD += 1;"),
    C(op="SETON", hi="LR"),
]

# Several diagnostics on one line: markers a, b, c.
M["LST035"] = ["**FREE", "//; run: no"] + HEAD[1:] + [
    "dcl-s a int(10);",
    "a = x + y + z;",
    "dcl-s q char(2) inz('abc'); dcl-s w int(5) inz(99999999);",
] + END

# Key lists, parameter lists, *ENTRY parameters, CALL.
M["LST036"] = [
    "      *; dds: CUSTF",
    "      *; sqlfile: CUSTF",
    "      *; run: no",
    comment("KLIST, PLIST, *ENTRY."),
    F("CUSTF", "I", desig="F", fmt="E", rtype="K", dev="DISK"),
    PRT_F,
    PRT_D,
    D("KEY", "S", to="5", typ="P", dec="0"),
    C(f1="*ENTRY", op="PLIST"),
    C(op="PARM", res="PARMIN", ln="5", dec="0"),
    C(f1="CK", op="KLIST"),
    C(op="KFLD", res="KEY"),
    C(op="Z-ADD", f2="PARMIN", res="KEY"),
    C(f1="CK", op="CHAIN", f2="CUSTR", hi="90"),
    C(op="CALL", f2="'LSTNONE'", res="PL", lo="91"),
    C(f1="PL", op="PLIST"),
    C(op="PARM", res="KEY"),
] + show("NAME + *IN90 + *IN91") + [C(op="SETON", hi="LR")]

# H specifications in fixed form, two lines; the options box.
M["LST037"] = [
    H("DFTACTGRP(*NO) ACTGRP(*NEW)"),
    H("DATFMT(*USA) TIMFMT(*HMS) DECEDIT('0,')"),
    "      *; run: no",
    PRT_F,
    PRT_D,
    D("D1", "S", typ="D", kw="INZ(D'2026-10-23')"),
] + show("%CHAR(D1)") + [C(op="SETON", hi="LR")]

# Listing directives: /TITLE, /SPACE, /EJECT.
M["LST038"] = [
    "      /TITLE Listing directives",
    "      *; run: no",
    PRT_F,
    PRT_D,
    "      /SPACE 3",
    D("N", "S", to="3", typ="P", dec="0", kw="INZ(7)"),
    "      /EJECT",
] + show("%CHAR(N)") + [C(op="SETON", hi="LR")]

# A diagnostic in a copied member, and one after the copy.
INC["LSTC4"] = ["**FREE", "dcl-s inside packed(3:0) inz(1234);"]
M["LST039"] = ["**FREE", "//; with: LSTC4", "//; run: no"] + HEAD[1:] + [
    "/copy LSTC4",
    "dcl-s after char(2) inz('xyz');",
] + p("%char(inside) + after") + END

# SQL-free CRTSQLRPGI: the precompiler's listing is not this one's.
M["LST040"] = ["**FREE", "//; run: no", "//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT) OPTION(*NOSECLVL *SHOWCPY *EXPDDS *EXT *SHOWSKP *SRCSTMT *DEBUGIO *UNREF)"] + HEAD[1:] + [
    "dcl-s n int(10) inz(1);",
] + p("%char(n)") + END

os.makedirs(OUT, exist_ok=True)
for name, lines in M.items():
    with open(os.path.join(OUT, name + ".rpgle"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
for name, lines in INC.items():
    with open(os.path.join(OUT, name + ".rpginc"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
print(len(M), "members,", len(INC), "brought members")
