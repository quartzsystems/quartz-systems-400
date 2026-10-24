"""Corpus batch 1, members 21 on: control, file, definition, input,
calculation, output and procedure specifications in fixed form with
their continuation forms; free form; directives; compile-time data.
b01.py calls add() and writes the members and copy members.
"""
import random

from fixed import C, CX, D, F, H, I_and, I_fld, I_rec, I_xfld, I_xrec, O_fld, O_rec, P, PRT_D, PRT_F, comment, free, place, show, wrap

INCS = {}
LR = CX("EVAL", "*INLR = *ON")
FH = ["**FREE", "ctl-opt dftactgrp(*no);", "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"]


def fp(expr, ind=""):
    """Print an expression from free form."""
    return [f"{ind}line = {expr};", f"{ind}write QSYSPRT line;"]


def fx(text):
    """A directive comment in fixed form."""
    return place([(7, "*; " + text)])


def cmt(line, text):
    """A record with a comment in columns 81 on."""
    return line.ljust(80) + text


def d(text):
    """A compiler directive in column 7."""
    return place([(7, text)])


def kw(text, col=44, spec="D"):
    """A keyword continuation line."""
    return place([(6, spec), (col, text)])


CUST = [fx("dds: CUSTF"), fx("sqlfile: CUSTF")]
CUSTE = F("CUSTF", "I", "F", fmt="E", rtype="K", dev="DISK")


def read_loop(fmt, body):
    """READ fmt until end of file (indicator 99) around body."""
    return [C(op="READ", f2=fmt, eq="99"), C("*IN99", "DOWEQ", "*OFF")] + body + [C(op="READ", f2=fmt, eq="99"), C(op="ENDDO")]


def add(member):
    # Control specifications over several lines; default formats.
    member("SPEC021", [
        H("DFTACTGRP(*NO)"),
        H("DATFMT(*USA) TIMFMT(*HMS)"),
        H("DECEDIT('0.')"),
        PRT_F, PRT_D,
        D("DT", "S", typ="D", kw="INZ(D'03/05/2026')"),
        D("TM", "S", typ="T", kw="INZ(T'14:30:05')"),
        D("N", "S", to="5", dec="2", kw="INZ(.25)"),
    ] + show("%CHAR(DT) + ' ' + %CHAR(TM) + ' ' + %CHAR(N)")
      + show("%CHAR(DT:*ISO) + ' ' + %CHAR(TM:*ISO)") + [LR])

    # Free-form control options in several statements over lines.
    member("SPEC022", [
        "**FREE",
        "ctl-opt dftactgrp(*no)",
        "        actgrp(*new);",
        "ctl-opt decedit(',')",
        "        datedit(*dmy.);",
        "dcl-f QSYSPRT printer(132);",
        "dcl-ds line len(132) end-ds;",
        "dcl-s n packed(7:2) inz(1234.5);",
        "dcl-s d8 zoned(8:0) inz(25122026);",
        "dcl-s d6 zoned(6:0) inz(251226);",
    ] + fp("%char(n) + ' ' + %editc(n : 'J')") + fp("%editc(d8 : 'Y') + ' ' + %editc(d6 : 'Y')") + ["*inlr = *on;"])

    # DATEDIT and the Y edit code in output specifications.
    member("SPEC023", [
        H("DFTACTGRP(*NO) DATEDIT(*YMD-)"),
        PRT_F,
        D("D8", "S", to="8", typ="S", dec="0", kw="INZ(20261225)"),
        D("D6", "S", to="6", typ="S", dec="0", kw="INZ(261225)"),
        C(op="EXCEPT", f2="DTL"),
        LR,
        O_rec("QSYSPRT", "E", exc="DTL", sa="1"),
        O_fld("D8", end="12", edit="Y"),
        O_fld("D6", end="24", edit="Y"),
    ])

    # A keyword's parameters continued on the next control line.
    member("SPEC024", [
        cmt(place([(6, "h"), (7, "dftactgrp(*no)")]), "first control line"),
        H("OPTION(*XREF:"),
        H("       *NODEBUGIO)"),
        H("DECEDIT('0,')"),
        PRT_F, PRT_D,
        D("N", "S", to="5", dec="2", kw="INZ(.5)"),
    ] + show("%CHAR(N) + ' ' + %CHAR(N * -3)") + [LR])

    # A linear main procedure.
    member("SPEC025", [
        "**FREE",
        "ctl-opt dftactgrp(*no) actgrp(*new) main(entry);",
        "dcl-f QSYSPRT printer(132);",
        "dcl-proc entry;",
        "  dcl-ds line len(132) end-ds;",
        "  dcl-s i int(10);",
        "  for i = 1 to 2;",
    ] + fp("'linear main ' + %char(i)", "    ") + [
        "  endfor;",
        "end-proc;",
    ])

    # EXPROPTS(*RESDECPOS) keeps the result's decimals in intermediates.
    member("SPEC026", FH[:1] + ["ctl-opt dftactgrp(*no) expropts(*resdecpos);"] + FH[2:] + [
        "dcl-s r packed(7:2);",
        "dcl-s a packed(5:0) inz(2);",
        "dcl-s b packed(5:0) inz(3);",
        "r = a / b * b;",
    ] + fp("%char(r)") + [
        "r = (a + 0.555) * b;",
    ] + fp("%char(r)") + ["*inlr = *on;"])

    # Control statements in mixed case.
    member("SPEC027", [
        "**FREE",
        "CTL-OPT DftActGrp(*No);",
        "Ctl-Opt Option(*NoDebugIO);",
        "dcl-f QSYSPRT printer(132);",
        "dcl-ds line len(132) end-ds;",
    ] + fp("'mixed case control'") + ["*inlr = *on;"])

    # INTPREC(20): integer intermediates of 20 digits.
    member("SPEC028", FH[:1] + ["ctl-opt dftactgrp(*no) intprec(20);"] + FH[2:] + [
        "dcl-s a int(10) inz(2000000000);",
        "dcl-s b int(20);",
        "b = a * 3;",
    ] + fp("%char(b)") + ["*inlr = *on;"])

    # A file keyword on a continuation line.
    member("SPEC029", [
        PRT_F,
        kw("OFLIND(*INOF)", spec="F"),
        PRT_D,
    ] + show("'of=' + *INOF") + [LR])

    # Program-described input: record and field lines.
    member("SPEC030", CUST + [
        F("CUSTF", "I", "F", fmt="F", rlen="40", dev="DISK"),
        PRT_F, PRT_D,
        I_rec("CUSTF", "NS", rid="01"),
        I_fld("CUSNO", "1", "3", dec="0", fmt="P"),
        I_fld("NAME", "4", "23"),
        I_fld("CITY", "24", "35"),
        I_fld("BAL", "36", "40", dec="2", fmt="P"),
    ] + read_loop("CUSTF", show("%CHAR(CUSNO) + ' ' + %TRIM(NAME) + '/' + %TRIM(CITY) + ' ' + %CHAR(BAL) + ' ' + *IN01")) + [LR])

    # PREFIX and RENAME on continuation lines.
    member("SPEC031", CUST + [
        CUSTE,
        kw("PREFIX(C_)", spec="F"),
        kw("RENAME(CUSTR:CREC)", spec="F"),
        PRT_F, PRT_D,
    ] + read_loop("CREC", show("%CHAR(C_CUSNO) + ' ' + %TRIM(C_NAME) + ' ' + %CHAR(C_BAL)")) + [LR])

    # A free-form file declaration over lines, with comments.
    member("SPEC032", [
        "**FREE",
        "//; dds: CUSTF",
        "//; sqlfile: CUSTF",
        "ctl-opt dftactgrp(*no);",
        "dcl-f custf            // the customer file",
        "      usage(*input)    // read only",
        "      keyed",
        "      rename(custr : cr)",
        "      prefix(k_);",
        "dcl-f QSYSPRT printer(132);",
        "dcl-ds line len(132) end-ds;",
        "chain (104) cr;",
        "if %found(custf);",
    ] + fp("%char(k_cusno) + ' ' + %trim(k_name)", "  ") + [
        "endif;",
        "chain (999) cr;",
        "if not %found(custf);",
    ] + fp("'999 not found'", "  ") + [
        "endif;",
        "*inlr = *on;",
    ])

    # USROPN, OPEN, CLOSE and %OPEN.
    member("SPEC033", CUST + [
        F("CUSTF", "I", "F", fmt="E", rtype="K", dev="DISK", kw="USROPN"),
        PRT_F, PRT_D,
        CX("EVAL", "*IN10 = %OPEN(CUSTF)"),
    ] + show("'before ' + *IN10") + [
        C(op="OPEN", f2="CUSTF"),
        CX("EVAL", "*IN10 = %OPEN(CUSTF)"),
        C(op="READ", f2="CUSTR"),
    ] + show("'open ' + *IN10 + ' ' + %TRIM(NAME)") + [
        C(op="CLOSE", f2="CUSTF"),
        CX("EVAL", "*IN10 = %OPEN(CUSTF)"),
    ] + show("'after ' + *IN10") + [LR])

    # An 80-byte printer in lower case, comments in columns 81 on.
    member("SPEC034", [
        cmt(F("qsysprt", "o", fmt="f", rlen="80", dev="printer"), "80-byte lines"),
        cmt(D("line", "ds", to="80"), "the line"),
        cmt(CX("eval", "line = 'eighty ' + %char(%size(line))"), "set it"),
        cmt(C(op="write", f2="qsysprt", res="line"), "print it"),
        CX("eval", "*inlr = *on"),
    ])

    # Comment and blank lines between a file's continuation lines; INFDS.
    member("SPEC035", CUST + [
        CUSTE,
        comment("the prefix comes next"),
        kw("PREFIX(P_)", spec="F"),
        "",
        kw("INFDS(FB)", spec="F"),
        PRT_F, PRT_D,
        D("FB", "DS"),
        place([(6, "D"), (7, " FNAME"), (26, "*FILE")]),
        place([(6, "D"), (7, " FSTAT"), (26, "*STATUS")]),
        C(op="READ", f2="CUSTR"),
    ] + show("FNAME + ' ' + %CHAR(FSTAT) + ' ' + %TRIM(P_NAME)") + [LR])

    # A qualified file and LIKEREC.
    member("SPEC036", [
        "**FREE",
        "//; dds: CUSTF",
        "//; sqlfile: CUSTF",
        "ctl-opt dftactgrp(*no);",
        "dcl-f custf keyed qualified;",
        "dcl-f QSYSPRT printer(132);",
        "dcl-ds line len(132) end-ds;",
        "dcl-ds rec likerec(custf.custr);",
        "read custf.custr rec;",
        "dow not %eof(custf);",
    ] + fp("%char(rec.cusno) + ' ' + %trim(rec.city) + ' ' + %char(%size(rec))", "  ") + [
        "  read custf.custr rec;",
        "enddo;",
        "*inlr = *on;",
    ])

    # A free-form printer declaration with output specifications.
    member("SPEC037", [
        free("dcl-f QSYSPRT printer(132) oflind(*inof);"),
        D("TOT", "S", to="7", dec="2", kw="INZ(12.5)"),
        C(op="EXCEPT", f2="LN"),
        LR,
        O_rec("QSYSPRT", "E", exc="LN", sa="1"),
        O_fld(const="'total'", end="10"),
        O_fld("TOT", end="25", edit="2"),
    ])

    # A name continued over three lines.
    member("SPEC038", [
        PRT_F, PRT_D,
        place([(6, "D"), (7, "THIS_IS_A_VERY_LONG_...")]),
        place([(6, "D"), (7, "NAME_SPLIT_OVER_...")]),
        D("THREE_LINES", "S", to="5", dec="0", kw="INZ(3)"),
        place([(6, "D"), (7, "ANOTHER_NAME_THAT_IS_LONG...")]),
        D("", "S", to="10", kw="INZ('second')"),
        free("line = %char(THIS_IS_A_VERY_LONG_NAME_SPLIT_OVER_THREE_LINES);"),
        C(op="WRITE", f2="QSYSPRT", res="LINE"),
        free("line = ANOTHER_NAME_THAT_IS_LONG;"),
        C(op="WRITE", f2="QSYSPRT", res="LINE"),
        LR,
    ])

    # Literals continued with + and - in the keyword area.
    member("SPEC039", [
        PRT_F, PRT_D,
        D("T1", "S", to="60", kw="VARYING INZ('Hello, +"),
        kw("   world')"),
        D("T2", "S", to="60", kw="VARYING INZ('ab-"),
        kw("   cd')"),
        D("T3", "S", to="60", kw="VARYING INZ('one +"),
        kw("two +", col=50),
        kw("three')"),
    ] + show("'[' + T1 + '] ' + %CHAR(%LEN(T1))") + show("'[' + T2 + '] ' + %CHAR(%LEN(T2))")
      + show("'[' + T3 + '] ' + %CHAR(%LEN(T3))") + [LR])

    # Named constants, one continued.
    member("SPEC040", [
        PRT_F, PRT_D,
        D("LONGK", "C", kw="'This constant is long and +"),
        kw("goes on to a second line'"),
        D("PI", "C", kw="3.14159"),
        D("NEG", "C", kw="CONST(-12)"),
        D("HEXK", "C", kw="X'C1C2C3'"),
        D("QUOT", "C", kw="'it''s'"),
        D("N2", "S", to="9", dec="5", kw="INZ(PI)"),
    ] + show("LONGK") + show("%CHAR(N2) + ' ' + %CHAR(NEG) + ' ' + HEXK + ' ' + QUOT")
      + show("%CHAR(%LEN(LONGK)) + ' ' + %CHAR(%LEN(QUOT))") + [LR])

    # A blank type: packed standalone, zoned subfield, character.
    member("SPEC041", [
        PRT_F, PRT_D,
        D("N1", "S", to="5", dec="2", kw="INZ(1.25)"),
        D("C1", "S", to="5", kw="INZ('ab')"),
        D("DS1", "DS"),
        D(" Z1", to="5", dec="2"),
        D(" X1", frm="6", to="8", dec="0"),
        D(" Y1", frm="9", to="12"),
        CX("EVAL", "Z1 = 12.34"),
        CX("EVAL", "X1 = 7"),
        CX("EVAL", "Y1 = 'abcd'"),
    ] + show("%CHAR(%SIZE(N1)) + ' ' + %CHAR(%SIZE(C1)) + ' ' + %CHAR(%SIZE(Z1)) + ' ' + %CHAR(%SIZE(DS1))")
      + show("DS1 + ' ' + %CHAR(N1) + ' [' + C1 + ']'") + [LR])

    # LIKE with a length adjustment in the length columns.
    member("SPEC042", [
        PRT_F, PRT_D,
        D("BASE", "S", to="7", dec="2", kw="INZ(1)"),
        D("BIG", "S", to="+3", kw="LIKE(BASE)"),
        D("SMALL", "S", to="-2", kw="LIKE(BASE)"),
        D("NM", "S", to="6", kw="INZ('abc')"),
        D("NM2", "S", to="+4", kw="LIKE(NM)"),
    ] + show("%CHAR(%LEN(BIG)) + ' ' + %CHAR(%DECPOS(BIG)) + ' ' + %CHAR(%SIZE(BIG))")
      + show("%CHAR(%LEN(SMALL)) + ' ' + %CHAR(%DECPOS(SMALL)) + ' ' + %CHAR(%SIZE(SMALL))")
      + show("%CHAR(%LEN(NM2)) + ' [' + NM2 + ']'") + [LR])

    # Graphic, UCS-2, varying, indicator, pointer, date, time, timestamp.
    member("SPEC043", [
        H("DFTACTGRP(*NO)"),
        PRT_F, PRT_D,
        D("G1", "S", to="5", typ="G"),
        D("U1", "S", to="5", typ="C"),
        D("V4", "S", to="10", typ="A", kw="VARYING(4)"),
        D("V2", "S", to="10", typ="A", kw="VARYING"),
        D("IND", "S", typ="N"),
        D("PT", "S", typ="*"),
        D("PP", "S", typ="*", kw="PROCPTR"),
        D("DT", "S", typ="D", kw="DATFMT(*EUR)"),
        D("TM", "S", typ="T", kw="TIMFMT(*USA)"),
        D("TS", "S", typ="Z"),
    ] + show("%CHAR(%SIZE(G1)) + ' ' + %CHAR(%LEN(G1)) + ' ' + %CHAR(%SIZE(U1)) + ' ' + %CHAR(%LEN(U1))")
      + show("%CHAR(%SIZE(V4)) + ' ' + %CHAR(%SIZE(V2)) + ' ' + %CHAR(%SIZE(IND)) + ' ' + %CHAR(%SIZE(PT))")
      + show("%CHAR(%SIZE(PP)) + ' ' + %CHAR(%SIZE(DT)) + ' ' + %CHAR(%SIZE(TM)) + ' ' + %CHAR(%SIZE(TS))") + [LR])

    # Lower-case definitions with indented subfield names.
    member("SPEC044", [
        PRT_F, PRT_D,
        D("rec", "ds", kw="qualified"),
        D("   code", to="3", kw="inz('abc')"),
        D("     qty", to="5", typ="p", dec="0", kw="inz(12)"),
        D("  flag", typ="n", kw="inz(*on)"),
    ] + show("rec.code + ' ' + %char(rec.qty) + ' ' + rec.flag + ' ' + %char(%size(rec))") + [LR])

    # A parameter's keywords on a continuation line; %PARMS.
    member("SPEC045", [
        H("DFTACTGRP(*NO)"),
        PRT_F, PRT_D,
        D("ADD3", "PR", to="10", typ="I", dec="0"),
        D("", to="10", typ="I", dec="0", kw="VALUE"),
        D("", to="10", typ="I", dec="0", kw="VALUE"),
        D("", to="10", typ="I", dec="0", kw="VALUE"),
        D("", kw="OPTIONS(*NOPASS)"),
    ] + show("%CHAR(ADD3(1:2)) + ' ' + %CHAR(ADD3(1:2:3))") + [
        LR,
        P("ADD3", "B"),
        D("", "PI", to="10", typ="I", dec="0"),
        D("A", to="10", typ="I", dec="0", kw="VALUE"),
        D("B", to="10", typ="I", dec="0", kw="VALUE"),
        D("C", to="10", typ="I", dec="0", kw="VALUE"),
        D("", kw="OPTIONS(*NOPASS)"),
        CX("IF", "%PARMS < 3"),
        CX("RETURN", "A + B"),
        C(op="ENDIF"),
        CX("RETURN", "A + B + C"),
        P("ADD3", "E"),
    ])

    # Externally described data structures.
    member("SPEC046", [fx("dds: CUSTF")] + [
        PRT_F, PRT_D,
        D("CUST", "DS", ext="E", kw="EXTNAME(CUSTF)"),
        D("CQ", "DS", ext="E", kw="EXTNAME(CUSTF) QUALIFIED"),
        CX("EVAL", "CUSNO = 5"),
        CX("EVAL", "NAME = 'Ann'"),
        CX("EVAL", "CQ.CUSNO = 6"),
    ] + show("%CHAR(%SIZE(CUST)) + ' ' + %CHAR(%SIZE(CQ)) + ' ' + %CHAR(%SIZE(CUSNO)) + ' ' + %CHAR(%SIZE(BAL))")
      + show("%CHAR(CUSNO) + ' ' + %TRIM(NAME) + ' ' + %CHAR(CQ.CUSNO)") + [LR])

    # Comments, blank lines and free comments among subfields.
    member("SPEC047", [
        PRT_F, PRT_D,
        D("REC", "DS"),
        comment("the first subfield"),
        cmt(D(" A", to="3", kw="INZ('aaa')"), "trailing comment"),
        "",
        D(" B", to="2", kw="INZ('bb')"),
        free("// a free-form comment between subfields"),
        D(" C", to="4", typ="S", dec="0", kw="INZ(12)"),
    ] + show("REC + ' ' + %CHAR(%SIZE(REC))") + [LR])

    # A keyword's parameter list split over two lines.
    member("SPEC048", [
        PRT_F, PRT_D,
        D("WHOLE", "DS"),
        D(" ALL", to="10", kw="INZ('abcdefghij')"),
        D(" MID", to="3", kw="OVERLAY(ALL:"),
        kw("5)"),
        D(" ARR", to="2", kw="DIM(5)"),
        kw("OVERLAY(ALL)"),
    ] + show("MID + ' ' + ARR(2) + ' ' + ARR(5) + ' ' + %CHAR(%ELEM(ARR))") + [LR])

    # LIKEDS and INZ(*LIKEDS) on a continuation line.
    member("SPEC049", [
        PRT_F, PRT_D,
        D("T", "DS", kw="QUALIFIED TEMPLATE"),
        D(" ID", to="5", typ="I", dec="0", kw="INZ(9)"),
        D(" NM", to="6", kw="INZ('tname')"),
        D("X", "DS", kw="LIKEDS(T)"),
        kw("INZ(*LIKEDS)"),
    ] + show("%CHAR(X.ID) + ' ' + X.NM + ' ' + %CHAR(%SIZE(X))") + [LR])

    # Every entry in lower case.
    member("SPEC050", [
        place([(6, "h"), (7, "dftactgrp(*no)")]),
        F("qsysprt", "o", fmt="f", rlen="132", dev="printer"),
        D("line", "ds", to="132"),
        D("n", "s", to="7", typ="p", dec="2", kw="inz(3.5)"),
        D("t", "s", to="8", typ="a", kw="inz('lower')"),
        CX("eval", "line = %char(n) + ' ' + %trim(t)"),
        C(op="write", f2="qsysprt", res="line"),
        CX("eval", "*inlr = *on"),
    ])

    # An externally described field renamed on an input line.
    member("SPEC051", CUST + [
        CUSTE, PRT_F, PRT_D,
        I_xrec("CUSTR"),
        I_xfld("NAME", "CNAME"),
    ] + read_loop("CUSTR", show("%CHAR(CUSNO) + ' ' + %TRIM(CNAME)")) + [LR])

    # Field indicators on an externally described field.
    member("SPEC052", CUST + [
        CUSTE, PRT_F, PRT_D,
        I_xrec("CUSTR"),
        I_xfld("BAL", plus="21", minus="22", zero="23"),
    ] + read_loop("CUSTR", show("%CHAR(CUSNO) + ' ' + *IN21 + *IN22 + *IN23")) + [LR])

    # Record identification codes with AND and OR lines.
    member("SPEC053", CUST + [
        F("CUSTF", "I", "F", fmt="F", rlen="40", dev="DISK"),
        PRT_F, PRT_D,
        I_rec("CUSTF", "AA", rid="01", codes=[("24", "", "C", "B")]),
        I_and([("25", "", "C", "o")], "AND"),
        I_fld("NAME", "4", "23"),
        I_rec("CUSTF", "AB", rid="02", codes=[("24", "", "C", "D")]),
        I_and([("24", "", "C", "A")], "OR"),
        I_fld("NAME", "4", "23"),
        I_rec("CUSTF", "AC", rid="03"),
        I_fld("NAME", "4", "23"),
    ] + read_loop("CUSTF", show("%TRIM(NAME) + ' ' + *IN01 + *IN02 + *IN03")) + [LR])

    # Result fields defined by length and decimals.
    member("SPEC054", [
        PRT_F, PRT_D,
        C(op="Z-ADD", f2="12.5", res="X", ln="5", dec="2"),
        C("'abc'", "CAT", "'def':1", "Y", ln="10"),
        C(op="MOVEL", f2="'xyz'", res="Z", ln="6"),
        C("X", "MULT", "2", "W", ln="7", dec="1"),
    ] + show("%CHAR(X) + ' [' + Y + '] [' + Z + '] ' + %CHAR(W) + ' ' + %CHAR(%SIZE(X))") + [LR])

    # String operations with factor 1 and factor 2 lists.
    member("SPEC055", [
        PRT_F, PRT_D,
        D("S", "S", to="20", kw="INZ('hello world')"),
        C("'o'", "SCAN", "S", "P1", ln="3", dec="0"),
        C("5", "SUBST", "S:3", "R1", ln="5"),
        C("'lo':'LO'", "XLATE", "S", "R2", ln="20"),
        C("'helo '", "CHECK", "S", "P2", ln="3", dec="0"),
        C("' '", "CHECKR", "S", "P3", ln="3", dec="0"),
    ] + show("%CHAR(P1) + ' [' + R1 + '] ' + %TRIM(R2) + ' ' + %CHAR(P2) + ' ' + %CHAR(P3)") + [LR])

    # An IF expression over three lines, a literal continued in an EVAL.
    member("SPEC056", [
        PRT_F, PRT_D,
        D("A", "S", to="5", dec="0", kw="INZ(4)"),
        D("B", "S", to="5", dec="0", kw="INZ(7)"),
        D("T", "S", to="30", kw="VARYING"),
        place([(6, "C"), (26, "IF"), (36, "A < B AND")]),
        place([(6, "C"), (36, "(B - A = 3 OR")]),
        place([(6, "C"), (36, "A = 0)")]),
        place([(6, "C"), (26, "EVAL"), (36, "T = 'it is +")]),
        place([(6, "C"), (40, "true'")]),
        C(op="ELSE"),
        CX("EVAL", "T = 'false'"),
        C(op="ENDIF"),
    ] + show("T") + [LR])

    # DOW and DOU with expressions; ITER and LEAVE.
    member("SPEC057", [
        PRT_F, PRT_D,
        D("I", "S", to="3", dec="0"),
        CX("DOW", "I < 10"),
        CX("EVAL", "I = I + 1"),
        CX("IF", "%REM(I:2) = 0"),
        C(op="ITER"),
        C(op="ENDIF"),
        CX("IF", "I > 7"),
        C(op="LEAVE"),
        C(op="ENDIF"),
    ] + show("'i=' + %CHAR(I)") + [
        C(op="ENDDO"),
        CX("DOU", "I >= 12"),
        CX("EVAL", "I = I + 2"),
        C(op="ENDDO"),
    ] + show("'end ' + %CHAR(I)") + [LR])

    # SELECT with WHEN, WHENxx, ANDxx and OTHER.
    member("SPEC058", [
        PRT_F, PRT_D,
        D("N", "S", to="3", dec="0"),
        C("1", "DO", "4", "N"),
        C(op="SELECT"),
        CX("WHEN", "N = 1"),
    ] + show("'one'") + [
        C("N", "WHENEQ", "2"),
    ] + show("'two'") + [
        C("N", "WHENGT", "2"),
        C("N", "ANDLT", "4"),
    ] + show("'three'") + [
        C(op="OTHER"),
    ] + show("'other ' + %CHAR(N)") + [
        C(op="ENDSL"),
        C(op="ENDDO"),
        LR,
    ])

    # CASxx to subroutines.
    member("SPEC059", [
        PRT_F, PRT_D,
        D("N", "S", to="3", dec="0"),
        C("1", "DO", "3", "N"),
        C("N", "CASEQ", "1", "SR1"),
        C("N", "CASEQ", "2", "SR2"),
        C(op="CAS", res="SR3"),
        C(op="ENDCS"),
        C(op="ENDDO"),
        LR,
        C("SR1", "BEGSR"),
    ] + show("'sr1'") + [
        C(op="ENDSR"),
        C("SR2", "BEGSR"),
    ] + show("'sr2'") + [
        C(op="ENDSR"),
        C("SR3", "BEGSR"),
    ] + show("'sr3 ' + %CHAR(N)") + [
        C(op="ENDSR"),
    ])

    # DO with an increment on ENDDO.
    member("SPEC060", [
        PRT_F, PRT_D,
        D("I", "S", to="3", dec="0"),
        C("1", "DO", "9", "I"),
    ] + show("'i=' + %CHAR(I)") + [
        C(op="ENDDO", f2="3"),
    ] + show("'after ' + %CHAR(I)") + [LR])

    # KLIST, KFLD and CHAIN with a not-found indicator.
    member("SPEC061", CUST + [
        CUSTE, PRT_F, PRT_D,
        D("KEY", "S", to="5", dec="0"),
        C("CKEY", "KLIST"),
        C(op="KFLD", res="KEY"),
        CX("EVAL", "KEY = 105"),
        C("CKEY", "CHAIN", "CUSTR", hi="90"),
        C(op="IF", f2="*IN90 = *OFF"),
    ] + show("%TRIM(NAME) + ' ' + %TRIM(CITY)") + [
        C(op="ENDIF"),
        CX("EVAL", "KEY = 999"),
        C("CKEY", "CHAIN", "CUSTR", hi="90"),
    ] + show("'nr=' + *IN90") + [LR])

    # Operation extenders and mixed-case operations; comment area.
    member("SPEC062", [
        PRT_F, PRT_D,
        D("R", "S", to="5", dec="2"),
        D("S", "S", to="5", dec="0"),
        cmt(CX("Eval(H)", "R = 2 / 3"), "rounded"),
        cmt(C(op="z-add(h)", f2="7.5", res="S"), "rounded too"),
    ] + show("%CHAR(R) + ' ' + %CHAR(S)") + [
        CX("eval", "R = 2 / 3"),
        C(op="Z-ADD", f2="7.5", res="S"),
    ] + show("%CHAR(R) + ' ' + %CHAR(S)") + [LR])

    # Compound assignment operators in fixed form.
    member("SPEC063", [
        PRT_F, PRT_D,
        D("N", "S", to="9", dec="2", kw="INZ(10)"),
        CX("EVAL", "N += 5"),
    ] + show("%CHAR(N)") + [CX("EVAL", "N -= 3")] + show("%CHAR(N)") + [CX("EVAL", "N *= 2")] + show("%CHAR(N)")
      + [CX("EVAL", "N /= 4")] + show("%CHAR(N)") + [CX("EVAL", "N **= 2")] + show("%CHAR(N)") + [LR])

    # CALLP and a call whose arguments continue over lines.
    member("SPEC064", [
        H("DFTACTGRP(*NO)"),
        PRT_F, PRT_D,
        D("CAT3", "PR", to="30", kw="VARYING"),
        D("", to="10", kw="VARYING CONST"),
        D("", to="10", kw="VARYING CONST"),
        D("", to="10", kw="VARYING CONST"),
        D("SHOWIT", "PR"),
        D("", to="40", kw="CONST"),
        place([(6, "C"), (26, "EVAL"), (36, "LINE = CAT3('alpha' :")]),
        place([(6, "C"), (36, "'beta' :")]),
        place([(6, "C"), (36, "'gamma')")]),
        C(op="WRITE", f2="QSYSPRT", res="LINE"),
        place([(6, "C"), (26, "CALLP"), (36, "SHOWIT('called ' +")]),
        place([(6, "C"), (36, "'with continuation')")]),
        LR,
        P("CAT3", "B"),
        D("", "PI", to="30", kw="VARYING"),
        D("A", to="10", kw="VARYING CONST"),
        D("B", to="10", kw="VARYING CONST"),
        D("C", to="10", kw="VARYING CONST"),
        CX("RETURN", "A + '-' + B + '-' + C"),
        P("CAT3", "E"),
        P("SHOWIT", "B"),
        D("", "PI"),
        D("T", to="40", kw="CONST"),
        CX("EVAL", "LINE = T"),
        C(op="WRITE", f2="QSYSPRT", res="LINE"),
        P("SHOWIT", "E"),
    ])

    # FOR with BY and DOWNTO.
    member("SPEC065", [
        PRT_F, PRT_D,
        D("I", "S", to="3", dec="0"),
        CX("FOR", "I = 1 TO 10 BY 3"),
    ] + show("'up ' + %CHAR(I)") + [
        C(op="ENDFOR"),
        CX("FOR", "I = 5 DOWNTO 1 BY 2"),
    ] + show("'down ' + %CHAR(I)") + [
        C(op="ENDFOR"),
        LR,
    ])

    # MOVE and MOVEL between character and numeric fields.
    member("SPEC066", [
        PRT_F, PRT_D,
        D("C10", "S", to="10", kw="INZ('abcdefghij')"),
        D("C4", "S", to="4", kw="INZ('wxyz')"),
        D("N5", "S", to="5", dec="0", kw="INZ(12345)"),
        C(op="MOVE", f2="C4", res="C10"),
    ] + show("C10") + [C(op="MOVEL", f2="C4", res="C10")] + show("C10")
      + [C(op="MOVEL(P)", f2="'ab'", res="C10")] + show("'[' + C10 + ']'")
      + [C(op="MOVE", f2="N5", res="C4")] + show("C4")
      + [C(op="MOVE", f2="'987'", res="N5")] + show("%CHAR(N5)") + [LR])

    # XFOOT, SORTA and LOOKUP over a compile-time array.
    member("SPEC067", [
        PRT_F, PRT_D,
        D("ARR", "S", to="3", dec="0", kw="DIM(5) CTDATA PERRCD(5)"),
        D("K", "S", to="3", dec="0", kw="INZ(9)"),
        D("I", "S", to="3", dec="0", kw="INZ(1)"),
        C(op="XFOOT", f2="ARR", res="TOT", ln="5", dec="0"),
        C(op="SORTA", f2="ARR"),
        C("K", "LOOKUP", "ARR(I)", eq="50"),
    ] + show("%CHAR(TOT) + ' ' + %CHAR(ARR(1)) + ' ' + %CHAR(ARR(5)) + ' ' + *IN50 + ' ' + %CHAR(I)") + [
        LR,
        "**CTDATA ARR",
        "005003009001007",
    ])

    # Indicators set by SETON and SETOFF; *IN(n).
    member("SPEC068", [
        PRT_F, PRT_D,
        C(op="SETON", hi="10", lo="11", eq="12"),
        C(op="SETOFF", hi="11"),
        CX("EVAL", "*IN(20) = *ON"),
        CX("EVAL", "*IN21 = *IN(10)"),
    ] + show("*IN10 + *IN11 + *IN12 + *IN20 + *IN(21)") + [LR])

    # DIV with MVR; SQRT.
    member("SPEC069", [
        PRT_F, PRT_D,
        C("17", "DIV", "5", "Q", ln="3", dec="0"),
        C(op="MVR", res="R", ln="3", dec="0"),
        C(op="SQRT", f2="16", res="S", ln="5", dec="2"),
        C("-7", "DIV", "2", "Q2", ln="3", dec="0"),
        C(op="MVR", res="R2", ln="3", dec="0"),
    ] + show("%CHAR(Q) + ' ' + %CHAR(R) + ' ' + %CHAR(S) + ' ' + %CHAR(Q2) + ' ' + %CHAR(R2)") + [LR])

    # COMP with high, low and equal indicators.
    member("SPEC070", [
        PRT_F, PRT_D,
        D("A", "S", to="3", dec="0", kw="INZ(5)"),
        D("B", "S", to="3", dec="0", kw="INZ(5)"),
        C("A", "COMP", "B", hi="10", lo="11", eq="12"),
    ] + show("'eq ' + *IN10 + *IN11 + *IN12") + [
        C("A", "COMP", "9", hi="10", lo="11", eq="12"),
    ] + show("'lt ' + *IN10 + *IN11 + *IN12") + [
        C("'b'", "COMP", "'a'", hi="10", lo="11", eq="12"),
    ] + show("'gt ' + *IN10 + *IN11 + *IN12") + [LR])

    # BITON, TESTB and TESTN.
    member("SPEC071", [
        PRT_F, PRT_D,
        D("CH", "S", to="1", kw="INZ(X'00')"),
        D("DG", "S", to="3", kw="INZ('123')"),
        D("BL", "S", to="3", kw="INZ(' 12')"),
        C(op="BITON", f2="'01'", res="CH"),
        C(op="TESTB", f2="'0'", res="CH", hi="21", eq="23"),
    ] + show("'b0 ' + *IN21 + *IN22 + *IN23") + [
        C(op="TESTB", f2="'01'", res="CH", hi="21", lo="22", eq="23"),
    ] + show("'b01 ' + *IN21 + *IN22 + *IN23") + [
        C(op="TESTB", f2="'012'", res="CH", hi="21", lo="22", eq="23"),
    ] + show("'b012 ' + *IN21 + *IN22 + *IN23") + [
        C(op="TESTN", res="DG", hi="31", lo="32", eq="33"),
    ] + show("'n ' + *IN31 + *IN32 + *IN33") + [
        C(op="TESTN", res="BL", hi="31", lo="32", eq="33"),
    ] + show("'nb ' + *IN31 + *IN32 + *IN33") + [LR])

    # CLEAR and RESET.
    member("SPEC072", [
        PRT_F, PRT_D,
        D("CNT", "S", to="5", dec="0", kw="INZ(42)"),
        D("NM", "S", to="8", kw="INZ('init')"),
        D("DS1", "DS"),
        D(" A", to="3", kw="INZ('aaa')"),
        D(" B", to="3", dec="0", kw="INZ(7)"),
        CX("EVAL", "CNT = 1"),
        CX("EVAL", "NM = 'chg'"),
        CX("EVAL", "A = 'zz'"),
        CX("EVAL", "B = 9"),
    ] + show("%CHAR(CNT) + ' ' + NM + ' ' + DS1") + [
        C(op="CLEAR", res="CNT"),
        C(op="CLEAR", res="NM"),
    ] + show("%CHAR(CNT) + ' [' + NM + '] ' + DS1") + [
        C(op="RESET", res="CNT"),
        C(op="RESET", res="NM"),
        C(op="RESET", res="DS1"),
    ] + show("%CHAR(CNT) + ' ' + NM + ' ' + DS1") + [LR])

    # EVALR and EVAL-CORR.
    member("SPEC073", [
        PRT_F, PRT_D,
        D("R", "S", to="10"),
        D("S1", "DS", kw="QUALIFIED"),
        D(" A", to="3", kw="INZ('one')"),
        D(" B", to="5", dec="0", kw="INZ(5)"),
        D(" C", to="2", kw="INZ('cc')"),
        D("S2", "DS", kw="QUALIFIED"),
        D(" B", to="7", dec="2", kw="INZ(1)"),
        D(" A", to="5", kw="INZ('x')"),
        D(" D", to="1", kw="INZ('d')"),
        CX("EVALR", "R = 'abc'"),
        CX("EVAL-CORR", "S2 = S1"),
    ] + show("'[' + R + '] ' + S2.A + ' ' + %CHAR(S2.B) + ' ' + S2.D") + [LR])

    # A hex literal and a literal with quotes, both continued.
    member("SPEC074", [
        PRT_F, PRT_D,
        D("T", "S", to="4"),
        D("T2", "S", to="30", kw="VARYING"),
        place([(6, "C"), (26, "EVAL"), (36, "T = X'C1C2+")]),
        place([(6, "C"), (36, "C3C4'")]),
        place([(6, "C"), (26, "EVAL"), (36, "T2 = 'it''s +")]),
        place([(6, "C"), (36, "Bob''s'")]),
    ] + show("T + ' ' + T2") + [LR])

    # Output constants continued with + and -.
    member("SPEC075", [
        PRT_F,
        C(op="EXCEPT", f2="LN"),
        LR,
        O_rec("QSYSPRT", "E", exc="LN", sa="1"),
        O_fld(const="'one +", end="40"),
        place([(6, "O"), (53, "two'")]),
        O_fld(const="'three-", end="60"),
        place([(6, "O"), (53, "   four'")]),
    ])

    # Edit words.
    member("SPEC076", [
        PRT_F,
        D("AMT", "S", to="9", dec="2", kw="INZ(-1234.56)"),
        D("AMT2", "S", to="9", dec="2", kw="INZ(5)"),
        C(op="EXCEPT", f2="LN"),
        LR,
        O_rec("QSYSPRT", "E", exc="LN", sa="1"),
        O_fld("AMT", end="30", const="' ,   ,  0.  -'"),
        O_fld("AMT2", end="60", const="' ,   ,  0.  CR'"),
        O_rec("QSYSPRT", "E", exc="LN", sa="1"),
        O_fld("AMT", end="30", const="'*   ,   .  '"),
        O_fld("AMT2", end="60", const="'   ,   0.  &CR'"),
    ])

    # Conditioning on output records and fields; an AND line.
    member("SPEC077", [
        PRT_F,
        C(op="SETON", hi="10"),
        C(op="SETOFF", hi="11", lo="12"),
        C(op="EXCEPT", f2="LN"),
        LR,
        O_rec("QSYSPRT", "E", cond=" 10", exc="LN", sa="1"),
        place([(6, "O"), (16, "AND"), (21, "N12")]),
        O_fld(const="'rec'", end="5"),
        O_fld(const="'ten'", end="10", cond=" 10"),
        O_fld(const="'eleven'", end="20", cond=" 11"),
        O_fld(const="'not11'", end="30", cond="N11"),
        O_rec("QSYSPRT", "E", cond=" 11", exc="LN", sa="1"),
        O_fld(const="'never'", end="10"),
        O_rec("QSYSPRT", "E", cond="N11", exc="LN", sa="1"),
        O_fld(const="'second'", end="10"),
    ])

    # Relative end positions and blank-after.
    member("SPEC078", [
        PRT_F,
        D("N", "S", to="5", dec="0", kw="INZ(77)"),
        D("S", "S", to="5", kw="INZ('abc')"),
        C(op="EXCEPT", f2="LN"),
        C(op="EXCEPT", f2="LN"),
        LR,
        O_rec("QSYSPRT", "E", exc="LN", sa="1"),
        O_fld("S", end="10", blank="B"),
        O_fld("N", end="+2", edit="1", blank="B"),
        O_fld(const="'x'", end="+1"),
    ])

    # Skip before, space after and PAGE.
    member("SPEC079", [
        PRT_F,
        C(op="EXCEPT", f2="HD"),
        C(op="EXCEPT", f2="DT"),
        C(op="EXCEPT", f2="DT"),
        C(op="EXCEPT", f2="HD"),
        C(op="EXCEPT", f2="DT"),
        LR,
        O_rec("QSYSPRT", "E", exc="HD", kb="3"),
        O_fld(const="'page'", end="10"),
        O_fld("PAGE", end="16", edit="Z"),
        O_rec("QSYSPRT", "E", exc="DT", sa="2"),
        O_fld(const="'detail'", end="10"),
    ])

    # Edit codes on output fields.
    codes = ["1", "2", "3", "4", "A", "B", "C", "D", "J", "K", "L", "M", "N", "O", "P", "Q", "X", "Z"]
    body = []
    for code in codes:
        body += [
            O_rec("QSYSPRT", "E", exc="ED", sa="1"),
            O_fld(const=f"'{code}'", end="3"),
            O_fld("NEG", end="25", edit=code),
            O_fld("ZER", end="45", edit=code),
            O_fld("POS", end="65", edit=code),
        ]
    member("SPEC080", [
        PRT_F,
        D("NEG", "S", to="7", dec="2", kw="INZ(-1234.5)"),
        D("ZER", "S", to="7", dec="2", kw="INZ(0)"),
        D("POS", "S", to="7", dec="2", kw="INZ(3.07)"),
        C(op="EXCEPT", f2="ED"),
        LR,
    ] + body)

    # A procedure name continued over lines.
    member("SPEC081", [
        H("DFTACTGRP(*NO)"),
        PRT_F, PRT_D,
        place([(6, "D"), (7, "COMPUTE_THE_TOTAL_OF_TWO_...")]),
        D("NUMBERS_PLEASE", "PR", to="10", typ="I", dec="0"),
        D("", to="10", typ="I", dec="0", kw="VALUE"),
        D("", to="10", typ="I", dec="0", kw="VALUE"),
        free("line = %char(COMPUTE_THE_TOTAL_OF_TWO_NUMBERS_PLEASE(40 : 2));"),
        C(op="WRITE", f2="QSYSPRT", res="LINE"),
        LR,
        place([(6, "P"), (7, "COMPUTE_THE_TOTAL_OF_TWO_...")]),
        P("NUMBERS_PLEASE", "B"),
        D("", "PI", to="10", typ="I", dec="0"),
        D("A", to="10", typ="I", dec="0", kw="VALUE"),
        D("B", to="10", typ="I", dec="0", kw="VALUE"),
        CX("RETURN", "A + B"),
        place([(6, "P"), (7, "COMPUTE_THE_TOTAL_OF_TWO_...")]),
        P("NUMBERS_PLEASE", "E"),
    ])

    # Local definitions; STATIC on a continuation line.
    member("SPEC082", [
        H("DFTACTGRP(*NO)"),
        PRT_F, PRT_D,
        D("COUNTER", "PR", to="10", typ="I", dec="0"),
        D("I", "S", to="3", dec="0"),
        C("1", "DO", "3", "I"),
    ] + show("%CHAR(COUNTER())") + [
        C(op="ENDDO"),
        LR,
        P("COUNTER", "B"),
        D("", "PI", to="10", typ="I", dec="0"),
        D("N", "S", to="10", typ="I", dec="0", kw="INZ(100)"),
        kw("STATIC"),
        D("M", "S", to="10", typ="I", dec="0", kw="INZ(100)"),
        CX("EVAL", "N = N + 1"),
        CX("EVAL", "M = M + 1"),
        CX("RETURN", "N * 1000 + M"),
        P("COUNTER", "E"),
    ])

    # A lower-case exported procedure.
    member("SPEC083", [
        H("DFTACTGRP(*NO)"),
        PRT_F, PRT_D,
        D("dbl", "pr", to="10", typ="i", dec="0"),
        D("", to="10", typ="i", dec="0", kw="value"),
    ] + show("%char(dbl(21))") + [
        LR,
        place([(6, "p"), (7, "dbl"), (24, "b"), (44, "export")]),
        D("", "pi", to="10", typ="i", dec="0"),
        D("n", to="10", typ="i", dec="0", kw="value"),
        CX("return", "n * 2"),
        place([(6, "p"), (7, "dbl"), (24, "e")]),
    ])

    # A subroutine inside a subprocedure.
    member("SPEC084", [
        H("DFTACTGRP(*NO)"),
        PRT_F, PRT_D,
        D("CALC", "PR", to="10", typ="I", dec="0"),
    ] + show("%CHAR(CALC())") + [
        LR,
        P("CALC", "B"),
        D("", "PI", to="10", typ="I", dec="0"),
        D("V", "S", to="10", typ="I", dec="0"),
        C(op="EXSR", f2="INIT"),
        CX("RETURN", "V"),
        C("INIT", "BEGSR"),
        CX("EVAL", "V = 21 * 2"),
        C(op="ENDSR"),
        P("CALC", "E"),
    ])

    # Free-form declarations inside a fixed-form procedure.
    member("SPEC085", [
        H("DFTACTGRP(*NO)"),
        PRT_F, PRT_D,
        D("TRIPLE", "PR", to="10", typ="I", dec="0"),
        D("", to="10", typ="I", dec="0", kw="VALUE"),
    ] + show("%CHAR(TRIPLE(14))") + [
        LR,
        P("TRIPLE", "B"),
        free("dcl-pi *n int(10);"),
        free("  n int(10) value;"),
        free("end-pi;"),
        free("dcl-s t int(10);"),
        free("t = n * 3;"),
        free("return t;"),
        P("TRIPLE", "E"),
    ])

    # A procedure that returns a structure.
    member("SPEC086", FH + [
        "dcl-ds pt qualified template;",
        "  x int(10);",
        "  y int(10);",
        "end-ds;",
        "dcl-pr mkpt likeds(pt);",
        "  a int(10) value;",
        "  b int(10) value;",
        "end-pr;",
        "dcl-ds p likeds(pt);",
        "p = mkpt(3 : 4);",
    ] + fp("%char(p.x) + ' ' + %char(p.y)") + [
        "*inlr = *on;",
        "dcl-proc mkpt;",
        "  dcl-pi *n likeds(pt);",
        "    a int(10) value;",
        "    b int(10) value;",
        "  end-pi;",
        "  dcl-ds r likeds(pt);",
        "  r.x = a;",
        "  r.y = b;",
        "  return r;",
        "end-proc;",
    ])

    # Comments everywhere in free form.
    member("SPEC087", [
        "**FREE",
        "// header comment",
        "ctl-opt dftactgrp(*no); // trailing",
        "dcl-f QSYSPRT printer(132); // printer",
        "dcl-ds line len(132) end-ds; // the line",
        "dcl-s a int(10)   // a comment inside a declaration",
        "      inz(5);     // ends here",
        "dcl-s s varchar(20) inz('a // not comment'); // literal holds //",
        "// a comment with a ; semicolon",
        "",
        "   // indented comment",
        "a = a     // first operand",
        "    + 2;  // second",
        "line = %char(a) + ' ' + s; // assign",
        "write QSYSPRT line;",
        "*inlr = *on; // done",
    ])

    # **free in lower case.
    member("SPEC088", [
        "**free",
        "ctl-opt dftactgrp(*no);",
        "dcl-f QSYSPRT printer(132);",
        "dcl-ds line len(132) end-ds;",
    ] + fp("'lower-case free marker'") + ["*inlr = *on;"])

    # Columns 1-5 hold sequence text; columns 81 on are comments.
    seq = [
        PRT_F, PRT_D,
        D("N", "S", to="5", dec="0", kw="INZ(4)"),
        cmt(free("n = n * 10;"), "junk; n = 0;"),
        cmt(free("line = 'n=' +"), "more junk"),
        cmt(free("       %char(n);"), "x"),
        C(op="WRITE", f2="QSYSPRT", res="LINE"),
        LR,
    ]
    member("SPEC089", [f"{(i + 1) * 100:05d}" + ln[5:] if len(ln) > 5 else f"{(i + 1) * 100:05d}" for i, ln in enumerate(seq)])

    # /free blocks between fixed calculations, in lower case.
    member("SPEC090", [
        PRT_F, PRT_D,
        D("X", "S", to="5", dec="0"),
        d("/free"),
        free("x = 5;"),
        d("/end-free"),
        CX("EVAL", "X = X + 1"),
        d("/FREE"),
        free("line = 'x=' + %char(x);"),
        free("write QSYSPRT line;"),
        d("/END-FREE"),
        LR,
    ])

    # DCL-SUBF for subfields named like operation codes.
    member("SPEC091", FH + [
        "dcl-ds rec;",
        "  dcl-subf read char(3) inz('rd');",
        "  dcl-subf select int(10) inz(4);",
        "  name char(5) inz('nm');",
        "end-ds;",
    ] + fp("%trim(read) + ' ' + %char(select) + ' ' + %trim(name) + ' ' + %char(%size(rec))") + ["*inlr = *on;"])

    # Literals continued in fully free form.
    member("SPEC092", FH + [
        "dcl-s s varchar(40);",
        "s = 'abc+",
        "     def';",
    ] + fp("'[' + s + ']'") + [
        "s = 'abc-",
        "   def';",
    ] + fp("'[' + s + ']'") + ["*inlr = *on;"])

    # A statement cut at every token.
    member("SPEC093", FH + [
        "dcl-s",
        "  x",
        "  packed(",
        "    7",
        "    :",
        "    2",
        "  )",
        "  inz(",
        "   1.5",
        "  );",
        "if",
        "  x",
        "  >",
        "  1",
        ";",
    ] + fp("%char(x)", "  ") + ["endif;", "*inlr = *on;"])

    # Special words and *OMIT.
    member("SPEC094", FH + [
        "dcl-pr f int(10);",
        "  a int(10) const options(*omit);",
        "end-pr;",
        "dcl-s p pointer inz(*null);",
        "dcl-s c char(4) inz(*all'ab');",
        "dcl-s n packed(5:2) inz(*hival);",
        "dcl-s m packed(5:2) inz(*loval);",
        "dcl-s i ind inz(*on);",
    ] + fp("%char(n) + ' ' + %char(m) + ' ' + c + ' ' + i") + [
        "if p = *null;",
    ] + fp("'null'", "  ") + [
        "endif;",
        "*in05 = *on;",
        "*in(6) = *in05;",
    ] + fp("*in05 + *in(6) + *inlr") + fp("%char(f(*omit)) + ' ' + %char(f(3))") + [
        "*inlr = *on;",
        "dcl-proc f;",
        "  dcl-pi *n int(10);",
        "    a int(10) const options(*omit);",
        "  end-pi;",
        "  if %addr(a) = *null;",
        "    return -1;",
        "  endif;",
        "  return a;",
        "end-proc;",
    ])

    # Long names and the characters # $ @.
    member("SPEC095", FH + [
        "dcl-s #count int(10) inz(3);",
        "dcl-s $amount packed(7:2) inz(1.5);",
        "dcl-s @at char(3) inz('at');",
        "dcl-s a_name_that_is_rather_long_for_an_rpg_variable_but_valid_x int(10) inz(77);",
        "dcl-s name_with_digits_123 int(10) inz(123);",
    ] + fp("%char(#count) + ' ' + %char($amount) + ' ' + @at") + fp("%char(a_name_that_is_rather_long_for_an_rpg_variable_but_valid_x)")
      + fp("%char(name_with_digits_123)") + ["*inlr = *on;"])

    # Names, operations and built-ins in any case.
    member("SPEC096", FH + [
        "DCL-S MyVar Int(10) Inz(4);",
        "myvar = MYVAR * 2;",
        "If MyVaR = 8;",
        "  Line = %Char(myVAR) + ' ' + %TRIM('  x ') + %trim(' y');",
        "  Write qsysprt LINE;",
        "EndIf;",
        "*INLR = *on;",
    ])

    # /TITLE, /EJECT and /SPACE in fixed form.
    member("SPEC097", [
        d("/TITLE A title for the listing"),
        PRT_F, PRT_D,
        d("/SPACE 2"),
        D("N", "S", to="3", dec="0", kw="INZ(1)"),
        d("/EJECT"),
    ] + show("'after directives ' + %CHAR(N)") + [d("/SPACE"), LR])

    # Listing directives in free form.
    member("SPEC098", [
        "**FREE",
        "/title The free-form title",
        "ctl-opt dftactgrp(*no);",
        "dcl-f QSYSPRT printer(132);",
        "dcl-ds line len(132) end-ds;",
        "/eject",
    ] + fp("'free directives'") + ["*inlr = *on;"])

    # /COPY of fixed-form definitions.
    INCS["SPC099C"] = [
        comment("Definitions for SPEC099."),
        D("CPYFLD", "S", to="10", kw="INZ('copied')"),
        D("CPYNUM", "S", to="5", dec="0", kw="INZ(321)"),
    ]
    member("SPEC099", [
        fx("with: SPC099C"),
        PRT_F, PRT_D,
        d("/COPY QRPGLESRC,SPC099C"),
    ] + show("CPYFLD + ' ' + %CHAR(CPYNUM)") + [LR])

    # /INCLUDE of a free-form member.
    INCS["SPC100I"] = [
        "**FREE",
        "dcl-s incfld char(8) inz('included');",
        "dcl-c INCK 42;",
    ]
    member("SPEC100", [
        "**FREE",
        "//; with: SPC100I",
        "ctl-opt dftactgrp(*no);",
        "dcl-f QSYSPRT printer(132);",
        "dcl-ds line len(132) end-ds;",
        "/include SPC100I",
    ] + fp("incfld + ' ' + %char(INCK)") + ["*inlr = *on;"])

    # Nested /COPY behind a guard, copied twice.
    INCS["SPC101A"] = [
        d("/IF NOT DEFINED(SPC101A)"),
        d("/DEFINE SPC101A"),
        D("GUARD", "S", to="10", typ="I", dec="0", kw="INZ(1)"),
        d("/COPY SPC101B"),
        d("/ENDIF"),
    ]
    INCS["SPC101B"] = [D("INNER", "S", to="6", kw="INZ('inner')")]
    member("SPEC101", [
        fx("with: SPC101A, SPC101B"),
        PRT_F, PRT_D,
        d("/COPY SPC101A"),
        d("/COPY SPC101A"),
    ] + show("%CHAR(GUARD) + ' ' + INNER") + [LR])

    # Conditional compilation in free form.
    member("SPEC102", FH + [
        "/define A",
        "/define B",
        "/if defined(A)",
        "/if defined(B)",
    ] + fp("'a and b'") + [
        "/else",
    ] + fp("'a only'") + [
        "/endif",
        "/undefine B",
        "/if defined(B)",
    ] + fp("'b still'") + [
        "/elseif defined(A)",
    ] + fp("'b gone'") + [
        "/else",
    ] + fp("'neither'") + [
        "/endif",
        "/endif",
        "*inlr = *on;",
    ])

    # The compiler's predefined conditions.
    checks = [("*ILERPG", True), ("*CRTBNDRPG", True), ("*CRTRPGMOD", False), ("*V7R5M0", True), ("*V6R1M0", True), ("*V7R6M0", False)]
    body = []
    for name, _ in checks:
        body += [d(f"/IF DEFINED({name})")] + show(f"'{name} yes'") + [d("/ELSE")] + show(f"'{name} no'") + [d("/ENDIF")]
    member("SPEC103", [PRT_F, PRT_D] + body + [LR])

    # Alternating compile-time arrays.
    member("SPEC104", [
        PRT_F, PRT_D,
        D("ABBR", "S", to="3", kw="DIM(4) CTDATA PERRCD(1)"),
        D("FULL", "S", to="10", kw="DIM(4) ALT(ABBR)"),
        D("I", "S", to="3", dec="0"),
        C("1", "DO", "4", "I"),
    ] + show("ABBR(I) + '=' + %TRIM(FULL(I))") + [
        C(op="ENDDO"),
        LR,
        "**CTDATA ABBR",
        "JANJanuary",
        "FEBFebruary",
        "MARMarch",
        "APRApril",
    ])

    # The ** form of compile-time data.
    member("SPEC105", [
        PRT_F, PRT_D,
        D("A1", "S", to="2", kw="DIM(3) CTDATA PERRCD(3)"),
        D("A2", "S", to="3", dec="0", kw="DIM(2) CTDATA PERRCD(1)"),
    ] + show("A1(1) + A1(2) + A1(3) + ' ' + %CHAR(A2(1) + A2(2))") + [
        LR,
        "**   the first array",
        "aabbcc",
        "**",
        "001",
        "002",
    ])

    # A table and its alternate with LOOKUP.
    member("SPEC106", [
        PRT_F, PRT_D,
        D("TABCOD", "S", to="1", kw="DIM(3) CTDATA PERRCD(1)"),
        D("TABNAM", "S", to="6", kw="DIM(3) ALT(TABCOD)"),
        C("'B'", "LOOKUP", "TABCOD", "TABNAM", eq="50"),
    ] + show("TABCOD + ' ' + TABNAM + ' ' + *IN50") + [
        C("'Z'", "LOOKUP", "TABCOD", "TABNAM", eq="50"),
    ] + show("TABCOD + ' ' + TABNAM + ' ' + *IN50") + [
        LR,
        "**CTDATA TABCOD",
        "ARed",
        "BBlue",
        "CCyan",
    ])

    # Compile-time data in a fully free member.
    member("SPEC107", FH + [
        "dcl-s days char(3) dim(7) ctdata perrcd(7);",
        "dcl-s i int(10);",
        "for i = 1 to %elem(days);",
    ] + fp("%char(i) + ' ' + days(i)", "  ") + [
        "endfor;",
        "*inlr = *on;",
        "**CTDATA days",
        "MONTUEWEDTHUFRISATSUN",
    ])

    # Numeric compile-time data with decimals.
    member("SPEC108", [
        PRT_F, PRT_D,
        D("AMT", "S", to="5", dec="2", kw="DIM(4) CTDATA PERRCD(2)"),
    ] + show("%CHAR(AMT(1)) + ' ' + %CHAR(AMT(2)) + ' ' + %CHAR(AMT(3)) + ' ' + %CHAR(AMT(4))") + [
        LR,
        "**CTDATA AMT",
        "0012500050",
        "1000099999",
    ])

    # Text after a record's entries is a comment.
    member("SPEC109", [
        PRT_F, PRT_D,
        D("CODES", "S", to="2", kw="DIM(4) CTDATA PERRCD(2)"),
    ] + show("CODES(1) + CODES(2) + CODES(3) + CODES(4)") + [
        LR,
        "**CTDATA CODES",
        "AABB  this rest is a comment",
        "CCDD  so is this",
    ])

    # A descending array and the LOOKUP built-ins.
    member("SPEC110", [
        PRT_F, PRT_D,
        D("LIM", "S", to="3", dec="0", kw="DIM(4) CTDATA PERRCD(4) DESCEND"),
        D("K", "S", to="3", dec="0", kw="INZ(250)"),
    ] + show("%CHAR(%LOOKUPLE(K:LIM)) + ' ' + %CHAR(%LOOKUPGE(K:LIM)) + ' ' + %CHAR(%LOOKUP(K:LIM))") + [
        LR,
        "**CTDATA LIM",
        "900500100010",
    ])

    # Free and fixed calculations mixed in a procedure.
    member("SPEC111", [
        H("DFTACTGRP(*NO)"),
        PRT_F, PRT_D,
        D("MIX", "PR"),
        C(op="CALLP", f2="MIX"),
        LR,
        P("MIX", "B"),
        D("N", "S", to="5", dec="0", kw="INZ(1)"),
        free("n += 1;"),
        C(op="ADD", f2="10", res="N"),
        free("line = 'mixed ' + %char(n);"),
        C(op="WRITE", f2="QSYSPRT", res="LINE"),
        P("MIX", "E"),
    ])

    # Indented free form in a fixed-format member, cut across lines.
    member("SPEC112", [
        PRT_F, PRT_D,
        free("dcl-s total packed(9:2) inz(0);"),
        free("dcl-s i int(10);"),
        free("for i = 1 to 4;"),
        cmt(free("      total = total"), "a"),
        cmt(free("            + i * 1.5;"), "b"),
        free("endfor;"),
        free("line ="),
        free("   'total=' +"),
        free("   %char(total);"),
        free("write QSYSPRT line;"),
        free("*inlr = *on;"),
    ])

    # Prototypes and interfaces with *N and END-PR on the same line.
    member("SPEC113", FH + [
        "dcl-pr getk int(10) end-pr;",
        "dcl-pr echo varchar(20);",
        "  *n varchar(20) const;",
        "end-pr;",
    ] + fp("%char(getk()) + ' ' + echo('hi')") + [
        "*inlr = *on;",
        "dcl-proc getk;",
        "  dcl-pi *n int(10) end-pi;",
        "  return 7;",
        "end-proc;",
        "dcl-proc echo;",
        "  dcl-pi *n varchar(20);",
        "    s varchar(20) const;",
        "  end-pi;",
        "  return s + '!';",
        "end-proc;",
    ])

    # END-DS, END-PI and END-PROC with names.
    member("SPEC114", FH + [
        "dcl-ds rec;",
        "  a char(2) inz('aa');",
        "end-ds rec;",
        "dcl-pr twice int(10);",
        "  n int(10) value;",
        "end-pr twice;",
    ] + fp("a + ' ' + %char(twice(8))") + [
        "*inlr = *on;",
        "dcl-proc twice;",
        "  dcl-pi twice int(10);",
        "    n int(10) value;",
        "  end-pi twice;",
        "  return n * 2;",
        "end-proc twice;",
    ])

    # Free and fixed definitions mixed.
    member("SPEC115", [
        PRT_F,
        free("dcl-ds line len(132) end-ds;"),
        D("A", "S", to="3", kw="INZ('fx1')"),
        free("dcl-s b char(3) inz('fr1');"),
        D("DS1", "DS"),
        D(" C", to="2", kw="INZ('cc')"),
        free("dcl-ds ds2;"),
        free("  e char(2) inz('ee');"),
        free("end-ds;"),
        D("F", "S", kw="LIKE(B) INZ('fx2')"),
    ] + show("A + B + DS1 + DS2 + F") + [LR])

    # A fully free member copies a fixed-form member.
    INCS["SPC116C"] = [
        D("FXA", "S", to="5", kw="INZ('fixed')"),
        D("FXN", "S", to="5", dec="0", kw="INZ(55)"),
    ]
    member("SPEC116", [
        "**FREE",
        "//; with: SPC116C",
        "ctl-opt dftactgrp(*no);",
        "dcl-f QSYSPRT printer(132);",
        "dcl-ds line len(132) end-ds;",
        "/copy SPC116C",
    ] + fp("fxa + ' ' + %char(fxn)") + ["*inlr = *on;"])

    # A fixed-form member copies a fully free member.
    INCS["SPC117C"] = [
        "**FREE",
        "dcl-s frc varchar(10) inz('free');",
        "dcl-ds frds qualified;",
        "  x int(10) inz(9);",
        "end-ds;",
    ]
    member("SPEC117", [
        fx("with: SPC117C"),
        PRT_F, PRT_D,
        d("/COPY SPC117C"),
    ] + show("FRC + ' ' + %CHAR(FRDS.X)") + [LR])

    # /EOF ends the copy member only.
    INCS["SPC118C"] = [
        D("BEFORE", "S", to="6", kw="INZ('before')"),
        d("/EOF"),
        "this is not RPG",
    ]
    member("SPEC118", [
        fx("with: SPC118C"),
        PRT_F, PRT_D,
        d("/COPY SPC118C"),
        D("AFTER", "S", to="5", kw="INZ('after')"),
    ] + show("BEFORE + ' ' + AFTER") + [LR])

    # /ELSEIF NOT DEFINED.
    member("SPEC119", [
        PRT_F, PRT_D,
        d("/DEFINE X1"),
        d("/IF DEFINED(X2)"),
    ] + show("'x2'") + [
        d("/ELSEIF NOT DEFINED(X3)"),
    ] + show("'not x3'") + [
        d("/ELSE"),
    ] + show("'else'") + [
        d("/ENDIF"),
        d("/IF NOT DEFINED(X1)"),
    ] + show("'no x1'") + [
        d("/ELSEIF DEFINED(X1)"),
    ] + show("'x1'") + [
        d("/ENDIF"),
        LR,
    ])

    # Typed literals in calculations.
    member("SPEC120", [
        PRT_F, PRT_D,
        D("DT", "S", typ="D"),
        D("TM", "S", typ="T"),
        D("TS", "S", typ="Z"),
        D("HX", "S", to="4"),
        CX("EVAL", "DT = D'2026-07-04'"),
        CX("EVAL", "TM = T'23.59.58'"),
        CX("EVAL", "TS = Z'2026-07-04-23.59.58.000001'"),
        CX("EVAL", "HX = X'C1C2C3C4'"),
    ] + show("%CHAR(DT) + ' ' + %CHAR(TM) + ' ' + HX") + show("%CHAR(TS)") + [LR])

    # A literal with doubled quotes continued in free form.
    member("SPEC121", FH + [
        "dcl-s s varchar(60);",
        "s = 'it''s +",
        "     Bob''s +",
        "     book';",
    ] + fp("s + ' ' + %char(%len(s))") + ["*inlr = *on;"])

    # A literal longer than a line.
    member("SPEC122", FH + [
        "dcl-s s varchar(200);",
        "s = 'The quick brown fox jumps over the lazy dog and keeps on running +",
        "     past the farm, over the hill and down to the river where it +",
        "     finally stops';",
    ] + fp("%subst(s : 1 : 60)") + fp("%subst(s : 61 : 60)") + fp("%subst(s : 121) + ' ' + %char(%len(s))") + ["*inlr = *on;"])

    # External indicators.
    member("SPEC123", [PRT_F, PRT_D] + show("*INU1 + *INU2 + *INU3 + *INU4 + *INU5 + *INU6 + *INU7 + *INU8") + [LR])

    # Figurative constants into fields of several types.
    member("SPEC124", FH + [
        "dcl-s c char(6);",
        "dcl-s v varchar(6) inz('abc');",
        "dcl-s z char(4);",
        "dcl-s d date;",
        "c = *allx'C1C2';",
        "v = *all'xy';",
        "z = *zeros;",
        "d = *hival;",
    ] + fp("c + ' [' + v + '] ' + z + ' ' + %char(d)") + [
        "d = *loval;",
        "c = *all'-';",
    ] + fp("%char(d) + ' ' + c") + ["*inlr = *on;"])

    # Built-ins in declarations.
    member("SPEC125", FH + [
        "dcl-c K 6;",
        "dcl-s base char(K) inz('base');",
        "dcl-s arr int(5) dim(K);",
        "dcl-s cpy char(%size(base));",
        "dcl-s arr2 packed(3:0) dim(%elem(arr));",
        "dcl-s sz int(10) inz(%size(arr));",
        "dcl-s ln int(10) inz(%len(base));",
    ] + fp("%char(%size(cpy)) + ' ' + %char(%elem(arr2)) + ' ' + %char(sz) + ' ' + %char(ln)") + ["*inlr = *on;"])

    # I/O operations cut over lines in free form.
    member("SPEC126", [
        "**FREE",
        "//; dds: CUSTF",
        "//; sqlfile: CUSTF",
        "ctl-opt dftactgrp(*no);",
        "dcl-f custf keyed;",
        "dcl-f QSYSPRT printer(132);",
        "dcl-ds line len(132) end-ds;",
        "chain",
        "  (103)",
        "  custr;",
    ] + fp("%trim(name)") + [
        "setll",
        "  *end",
        "  custf;",
        "readp",
        "  custf;",
    ] + fp("%trim(name)") + [
        "setll *start custf;",
        "read custf;",
    ] + fp("%trim(name)") + ["*inlr = *on;"])

    # MONITOR, ON-ERROR and ENDMON in fixed form.
    member("SPEC127", [
        PRT_F, PRT_D,
        D("A", "S", to="5", dec="0", kw="INZ(10)"),
        D("B", "S", to="5", dec="0", kw="INZ(0)"),
        D("R", "S", to="5", dec="0"),
        C(op="MONITOR"),
        CX("EVAL", "R = A / B"),
    ] + show("'no error'") + [
        C(op="ON-ERROR", f2="00102"),
    ] + show("'divide by zero'") + [
        C(op="ON-ERROR"),
    ] + show("'other'") + [
        C(op="ENDMON"),
    ] + show("'status ' + %CHAR(%STATUS)") + [LR])

    # Every specification type in one member.
    member("SPEC128", CUST + [
        H("DFTACTGRP(*NO)"),
        CUSTE,
        PRT_F,
        D("CNT", "S", to="3", dec="0"),
        D("BUMP", "PR", to="3", dec="0"),
        D("", to="3", dec="0", kw="VALUE"),
        I_xrec("CUSTR"),
        I_xfld("CITY", "TOWN"),
    ] + read_loop("CUSTR", [CX("EVAL", "CNT = BUMP(CNT)"), C(op="EXCEPT", f2="LN")]) + [
        LR,
        O_rec("QSYSPRT", "E", exc="LN", sa="1"),
        O_fld("CNT", end="4", edit="Z"),
        O_fld("TOWN", end="20"),
        P("BUMP", "B"),
        D("", "PI", to="3", dec="0"),
        D("N", to="3", dec="0", kw="VALUE"),
        CX("RETURN", "N + 1"),
        P("BUMP", "E"),
    ])

    # Named constants of every literal form.
    member("SPEC129", FH + [
        "dcl-c CHR 'text';",
        "dcl-c NUM -42.5;",
        "dcl-c HEX x'C8C5E7';",
        "dcl-c DAT d'2026-01-31';",
        "dcl-c TIM t'08.15.00';",
        "dcl-c BIG 123456789012345;",
        "dcl-c FLT 1.5E2;",
        "dcl-c ONE const(1);",
        "dcl-c IND *on;",
        "dcl-c BLK *blanks;",
        "dcl-s v varchar(10) inz(BLK);",
        "dcl-s d date inz(DAT);",
        "dcl-s f float(8) inz(FLT);",
    ] + fp("CHR + ' ' + %char(NUM) + ' ' + HEX + ' ' + %char(d) + ' ' + %char(TIM)")
      + fp("%char(BIG + ONE) + ' ' + %char(f) + ' [' + v + '] ' + %char(%len(v))") + ["*inlr = *on;"])

    # A free-form data structure with keywords over lines.
    member("SPEC130", FH + [
        "dcl-ds rec",
        "       qualified",
        "       inz;",
        "  code",
        "    char(3)",
        "    inz('xyz');",
        "  qty packed(5:0)",
        "      inz(12);",
        "  arr",
        "    int(5)",
        "    dim(3);",
        "end-ds;",
    ] + fp("rec.code + ' ' + %char(rec.qty) + ' ' + %char(rec.arr(2)) + ' ' + %char(%size(rec))") + ["*inlr = *on;"])

    rng = random.Random(1021)
    for n in range(131, 154):
        member(f"SPEC{n:03d}", layout(rng))


NAMES = ["CUSTOMER_NAME_FIELD", "TOTAL_AMOUNT_DUE_TODAY", "A_COUNTER", "LONGER_NAME_FOR_A_DATE_VALUE", "TX", "SHORTNM", "THE_QUANTITY_ON_HAND"]
WORDS = ["alpha beta", "the quick brown fox", "one, two, three", "keep  two  blanks", "x", "a fairly long piece of text"]


def mixcase(rng, text):
    """Upper, lower or mixed case outside literals."""
    mode = rng.choice(["u", "l", "m"])
    out, quote = [], False
    for ch in text:
        if ch == "'":
            quote = not quote
        if quote or mode == "u":
            out.append(ch)
        elif mode == "l":
            out.append(ch.lower())
        else:
            out.append(ch.lower() if rng.random() < 0.5 else ch.upper())
    return "".join(out)


def dname(rng, name, rest):
    """A definition with its name, continued with ... when long or by chance."""
    if len(name) < 3 or len(name) <= 15 and rng.random() < 0.7:
        return [D(name, **rest)]
    cut = rng.randint(max(1, len(name) - 15), len(name) - 1)
    return [place([(6, "D"), (7, name[:cut] + "...")])] + [D(name[cut:], **rest)]


def inz_lines(rng, text):
    """INZ of a character literal, maybe continued with + or -."""
    if len(text) < 4 or rng.random() < 0.3:
        return [f"INZ('{text}')"], []
    k = rng.randint(1, len(text) - 1)
    if rng.random() < 0.5 and text[k] != " ":
        return [f"INZ('{text[:k]}+"], [kw(" " * rng.randint(0, 6) + text[k:] + "')")]
    return [f"INZ('{text[:k]}-"], [kw(text[k:] + "')")]


def layout(rng):
    """One program in a random fixed-form layout: names continued, keywords
    on continuation lines, literals continued, expressions cut at random
    widths, any case, comments between statements."""
    names = rng.sample(NAMES, 4)
    lines = [PRT_F, PRT_D]
    words = rng.choice(WORDS)
    # A character field.
    first, more = inz_lines(rng, words)
    if rng.random() < 0.5 and len(first[0]) <= 28:
        lines += dname(rng, names[0], dict(deftype="S", to="40", kw="VARYING " + first[0])) + more
    else:
        lines += dname(rng, names[0], dict(deftype="S", to="40", kw="VARYING")) + [kw(first[0])] + more
    # A packed field and a zoned one, keywords maybe on their own lines.
    val = f"{rng.randint(0, 9999)}.{rng.randint(0, 99):02d}"
    if rng.random() < 0.5:
        lines += dname(rng, names[1], dict(deftype="S", to="9", typ="P", dec="2", kw=f"INZ({val})"))
    else:
        lines += dname(rng, names[1], dict(deftype="S", to="9", typ="P", dec="2")) + [kw(f"INZ({val})")]
    cnt = rng.randint(1, 50)
    lines += dname(rng, names[2], dict(deftype="S", to="5", typ="S", dec="0", kw=f"INZ({cnt})"))
    lines += dname(rng, names[3], dict(deftype="S", to="3", kw=f"DIM({rng.randint(2, 5)})")) + [kw("INZ('ab')")]

    def filler():
        k = rng.random()
        if k < 0.15:
            return [comment("a comment line")]
        if k < 0.25:
            return [""]
        if k < 0.35:
            return [free("// a free-form comment")]
        return []

    def eval_(text):
        pieces = wrap(text, rng.randint(12, 41))
        out = [place([(6, "C"), (26, mixcase(rng, "EVAL")), (36, pieces[0])])]
        out += [place([(6, "C"), (36 + rng.randint(0, 3), p)]) for p in pieces[1:]]
        if rng.random() < 0.3:
            out[0] = cmt(out[0], "comment area")
        return out

    def write():
        return [mixcase(rng, C(op="WRITE", f2="QSYSPRT", res="LINE"))]

    calc = []
    calc += filler() + eval_(mixcase(rng, f"{names[2]} = {names[2]} * 2 + 1"))
    calc += filler() + eval_(mixcase(rng, f"LINE = '[' + {names[0]} + '] ' + %CHAR(%LEN({names[0]}))")) + write()
    calc += filler() + eval_(mixcase(rng, f"LINE = %CHAR({names[1]}) + ' ' + %CHAR({names[2]}) + ' ' + %CHAR(%ELEM({names[3]}))")) + write()
    calc += filler() + eval_(mixcase(rng, f"{names[1]} = {names[1]} + {names[2]} / 4")) + eval_(mixcase(rng, f"LINE = %CHAR({names[1]}) + ' ' + {names[3]}(2)")) + write()
    calc += [CX(mixcase(rng, "EVAL"), mixcase(rng, "*INLR = *ON"))]
    return lines + calc

