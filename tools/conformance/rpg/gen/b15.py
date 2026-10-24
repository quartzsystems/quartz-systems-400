# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.

"""Corpus batch 15: coverage of the operation codes, built-in functions
and keywords no other batch verifies (gen/matrix.py lists them), a
valid use of each and its error paths. Writes ../b15cov/COVnnn.rpgle
and the members they bring.

usage: python -I b15.py
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fixed import C, CX, D, F, H, O_fld, O_rec, P, PRT_D, PRT_F, show  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b15cov")
HEAD = [
    "**FREE",
    "ctl-opt dftactgrp(*no);",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
]
M = {}
INC = {}


def p(expr, ind=""):
    return [f"{ind}line = {expr};", f"{ind}write QSYSPRT line;"]


def member(name, decls, body, tail=None, head=None, dirs=()):
    h = head if head is not None else HEAD
    M[name] = h[:1] + list(dirs) + h[1:] + decls + body + (tail if tail is not None else ["*inlr = *on;"])


def fixed(name, decls, body, dirs=(), tail=(), hspec=None):
    lines = list(dirs) + ([H(hspec)] if hspec else []) + [PRT_F, PRT_D] + decls + body
    lines += [CX("EVAL", "*INLR = *ON")] + list(tail)
    M[name] = lines


def guard(stmts, label, codes="", ind=""):
    """A monitor around stmts that prints the status it ends with."""
    on = f"on-error {codes};" if codes else "on-error;"
    return [f"{ind}monitor;"] + [ind + "  " + s for s in stmts] + p(f"'{label} ok'", ind + "  ") + [ind + on] \
        + p(f"'{label} ' + %char(%status)", ind + "  ") + [ind + "endmon;"]


# ---- Operation codes ---------------------------------------------------

# ACQ in a batch job: no device to acquire.
member("COV001", ["dcl-f B8A workstn usropn maxdev(*file) devid(dev);", "dcl-s dev char(10) inz('none');"], [
    "acq(e) 'COVNODEV' B8A;",
] + p("'acq ' + %char(%status(B8A)) + ' ' + %error + ' ' + dev")
  + guard(["acq 'COVNODEV' B8A;"], "acq mon"), dirs=["//; dds: B8A"])

# REL in a batch job.
member("COV002", ["dcl-f B8A workstn usropn maxdev(*file);"], [
    "rel(e) 'COVNODEV' B8A;",
] + p("'rel ' + %char(%status(B8A)) + ' ' + %error")
  + guard(["rel 'COVNODEV' B8A;"], "rel mon"), dirs=["//; dds: B8A"])

# NEXT in a batch job.
member("COV003", ["dcl-f B8A workstn usropn maxdev(*file);"], [
    "next(e) 'COVNODEV' B8A;",
] + p("'next ' + %char(%status(B8A)) + ' ' + %error"), dirs=["//; dds: B8A"])

# POST of a printer file: the INFDS gets the file's feedback.
member("COV004", [
    "dcl-f QPRINT printer(132) infds(fb) usropn;",
    "dcl-ds fb;",
    "  fname *file;",
    "  fsts *status;",
    "  fop *opcode;",
    "end-ds;",
], [
    "open QPRINT;",
    "post QPRINT;",
] + p("'post ' + fname + ' ' + %char(fsts) + ' ' + fop") + [
    "close QPRINT;",
    "post(e) QPRINT;",
] + p("'closed ' + %char(%status) + ' ' + %error"))

# ALLOC and REALLOC (fixed form): the data moves with the storage.
fixed("COV005", [
    D("PTR", "S", typ="*"),
    D("BUF", "S", to="300", kw="BASED(PTR)"),
    D("N", "S", to="10", typ="I", dec="0"),
], [
    C(op="ALLOC", f2="100", res="PTR"),
    CX("EVAL", "%SUBST(BUF:1:5) = 'hello'"),
    C(op="REALLOC", f2="300", res="PTR"),
    CX("EVAL", "%SUBST(BUF:290:3) = 'end'"),
    *show("'alloc ' + %SUBST(BUF:1:5) + ' ' + %SUBST(BUF:290:3)"),
    C(op="DEALLOC", res="PTR"),
    CX("IF", "PTR = *NULL"),
    *show("'null after dealloc'"),
    C(op="ENDIF"),
])

# ALLOC and REALLOC errors: no storage of length 0, a null pointer.
fixed("COV006", [
    D("PTR", "S", typ="*"),
    D("PTR2", "S", typ="*"),
    D("N", "S", to="10", typ="I", dec="0", kw="INZ(0)"),
], [
    C(op="ALLOC(E)", f2="N", res="PTR"),
    *show("'alloc 0 ' + %CHAR(%STATUS) + ' ' + %ERROR"),
    C(op="REALLOC(E)", f2="50", res="PTR2"),
    *show("'realloc null ' + %CHAR(%STATUS) + ' ' + %ERROR"),
    C(op="ALLOC", f2="10", res="PTR"),
    C(op="REALLOC(E)", f2="N", res="PTR"),
    *show("'realloc 0 ' + %CHAR(%STATUS) + ' ' + %ERROR"),
])

# CALLB of a procedure of the module, by name and by pointer.
fixed("COV007", [
    D("N", "S", to="10", typ="I", dec="0", kw="INZ(41)"),
    D("PP", "S", typ="*", kw="PROCPTR INZ(%PADDR('TWICE'))"),
], [
    C(op="CALLB", f2="'ADDONE'"),
    C(op="PARM", res="N"),
    *show("'addone ' + %CHAR(N)"),
    C(op="CALLB", f2="PP"),
    C(op="PARM", res="N"),
    *show("'twice ' + %CHAR(N)"),
], tail=[
    P("ADDONE", "B"),
    D("ADDONE", "PI"),
    D(" X", to="10", typ="I", dec="0"),
    CX("EVAL", "X = X + 1"),
    P("ADDONE", "E"),
    P("TWICE", "B"),
    D("TWICE", "PI"),
    D(" X", to="10", typ="I", dec="0"),
    CX("EVAL", "X = X * 2"),
    P("TWICE", "E"),
])

# CALLB of a procedure nothing exports: the program is not bound.
fixed("COV008", [D("N", "S", to="10", typ="I", dec="0")], [
    C(op="CALLB", f2="'COVNOPROC'"),
    C(op="PARM", res="N"),
    *show("'called'"),
])

# CASxx and ENDCS, with CAS for the rest and resulting indicators.
fixed("COV009", [D("N", "S", to="3", typ="P", dec="0"), D("RES", "S", to="20")], [
    C(f1="1", op="DO", f2="4", res="N"),
    C(f1="N", op="CASEQ", f2="1", res="ONE"),
    C(f1="N", op="CASGT", f2="3", res="BIG"),
    C(op="CAS", res="REST"),
    C(op="ENDCS"),
    *show("%CHAR(N) + ' ' + RES"),
    C(op="ENDDO"),
    C(f1="N", op="CASLT", f2="9", res="ONE", hi="51", lo="52", eq="53"),
    C(op="END"),
    *show("'ind ' + *IN51 + *IN52 + *IN53 + ' ' + RES"),
], tail=[
    C(f1="ONE", op="BEGSR"),
    CX("EVAL", "RES = 'one'"),
    C(op="ENDSR"),
    C(f1="BIG", op="BEGSR"),
    CX("EVAL", "RES = 'big'"),
    C(op="ENDSR"),
    C(f1="REST", op="BEGSR"),
    CX("EVAL", "RES = 'rest'"),
    C(op="ENDSR"),
])

# CASxx naming a subroutine that does not exist.
fixed("COV010", [D("N", "S", to="3", typ="P", dec="0", kw="INZ(1)")], [
    C(f1="N", op="CASEQ", f2="1", res="NOSUCH"),
    C(op="ENDCS"),
    *show("'after'"),
])

# CASxx without ENDCS.
fixed("COV011", [D("N", "S", to="3", typ="P", dec="0", kw="INZ(1)")], [
    C(f1="N", op="CASEQ", f2="1", res="SR1"),
    *show("'after'"),
], tail=[C(f1="SR1", op="BEGSR"), C(op="ENDSR")])

# COMMIT and ROLBK with the job's commitment control started.
member("COV012", [], [
    "commit(e);",
] + p("'commit ' + %char(%status) + ' ' + %error") + [
    "rolbk(e);",
] + p("'rolbk ' + %char(%status) + ' ' + %error") + [
    "commit(e) 'boundary1';",
] + p("'commit id ' + %char(%status) + ' ' + %error"),
    head=["**FREE", "ctl-opt dftactgrp(*no) actgrp(*caller);", "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"],
    dirs=["//; setup: STRCMTCTL LCKLVL(*CHG) CMTSCOPE(*JOB)", "//; cleanup: ENDCMTCTL"])

# COMMIT and ROLBK with no commitment control: status 802.
member("COV013", [], [
    "commit(e);",
] + p("'commit ' + %char(%status) + ' ' + %error") + [
    "rolbk(e);",
] + p("'rolbk ' + %char(%status) + ' ' + %error") + guard(["commit;"], "commit mon"))

# DATA-GEN with a generator program that does not exist.
member("COV014", [
    "dcl-ds info qualified;",
    "  name varchar(10) inz('bolt');",
    "  qty int(10) inz(3);",
    "end-ds;",
    "dcl-s out varchar(200);",
], guard(["data-gen info %data(out:'doc=string') %gen('COVNOGEN');"], "gen")
  + p("'out [' + out + ']'"))

# DATA-INTO with a parser program that does not exist.
member("COV015", [
    "dcl-ds info qualified;",
    "  name varchar(10);",
    "end-ds;",
], guard(["data-into info %data('name=x':'doc=string') %parser('COVNOPRS');"], "into"))

# DATA-INTO with %HANDLER and a parser that does not exist.
member("COV016", [
    "dcl-ds item qualified template;",
    "  name varchar(10);",
    "end-ds;",
    "dcl-s count int(10);",
], guard(["data-into %handler(got:count) %data('x':'doc=string') %parser('COVNOPRS':'opt');"], "into")
  + p("'count ' + %char(count)"), tail=[
    "*inlr = *on;",
    "dcl-proc got;",
    "  dcl-pi *n int(10);",
    "    n int(10);",
    "    items likeds(item) dim(5) const;",
    "    num int(10) value;",
    "  end-pi;",
    "  n += num;",
    "  return 0;",
    "end-proc;",
])

# DEFINE (fixed form): *LIKE with a length change, and *DTAARA.
fixed("COV017", [
    D("FLD", "S", to="5", kw="INZ('abcde')"),
    D("NUM", "S", to="5", typ="P", dec="2", kw="INZ(123.45)"),
], [
    C(f1="*LIKE", op="DEFINE", f2="FLD", res="LONGER", ln="+3"),
    C(f1="*LIKE", op="DEFINE", f2="NUM", res="SHORTN", ln="-2"),
    C(f1="*LIKE", op="DEFINE", f2="FLD", res="SAME"),
    CX("EVAL", "LONGER = FLD + 'xyz'"),
    CX("EVAL", "SHORTN = 4.5"),
    *show("LONGER + ' ' + %CHAR(%LEN(LONGER)) + ' ' + %CHAR(%LEN(SHORTN)) + ' ' + %EDITC(SHORTN:'X') + ' ' + %CHAR(%LEN(SAME))"),
])

# DEFINE errors: *LIKE of a name that is not defined.
fixed("COV018", [], [
    C(f1="*LIKE", op="DEFINE", f2="NOSUCH", res="NEWF", ln="+1"),
    *show("'x'"),
])

# DSPLY without a response in a batch job.
member("COV019", ["dcl-s n int(10) inz(42);"], [
    "dsply 'covered message';",
    "dsply ('n is ' + %char(n));",
    "dsply n;",
] + p("'after dsply'"))

# DUMP without DEBUG(*YES) does nothing. (DUMP(A) dumps anyway; its
# QPPGMDMP holds the job, dates and spooled file names, so no member
# prints one.)
member("COV020", ["dcl-s n int(10) inz(7);"], [
    "dump;",
    "dump 'one';",
] + p("'after dump ' + %char(n)"),
    head=["**FREE", "ctl-opt dftactgrp(*no) debug(*no);", "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"])

# FORCE in the cycle: the next record comes from the secondary file.
M["COV021"] = [
    "      *; dds: MSTF",
    "      *; sqlfile: MSTF",
    "      *; dds: ACTF",
    "      *; sqlfile: ACTF",
    "     FMSTF      IP   F   13        DISK",
    "     FACTF      IS   F    8        DISK",
    PRT_F,
    PRT_D,
    "     IMSTF      AA  01",
    "     I                                  1    3  MCUS",
    "     I                                  4   13  MNAME",
    "     IACTF      BB  02",
    "     I                                  1    3  ACUS",
    "     I                                  4    8 0AAMT",
    CX("EVAL", "LINE = 'master ' + MCUS + ' ' + MNAME", cond=" 01"),
    C(cond=" 01", op="WRITE", f2="QSYSPRT", res="LINE"),
    C(cond=" 01", op="FORCE", f2="ACTF"),
    CX("EVAL", "LINE = 'activity ' + ACUS", cond=" 02"),
    C(cond=" 02", op="WRITE", f2="QSYSPRT", res="LINE"),
]

# IN and OUT of a data area the setup made, *LOCK and UNLOCK.
member("COV022", [
    "dcl-s da char(20) dtaara('COVDA1');",
], [
    "in da;",
] + p("'in [' + da + ']'") + [
    "in *lock da;",
    "da = 'changed';",
    "out da;",
    "da = *blanks;",
    "in da;",
] + p("'again [' + da + ']'"), dirs=[
    "//; setup: CRTDTAARA DTAARA({lib}/COVDA1) TYPE(*CHAR) LEN(20) VALUE('first value')",
    "//; cleanup: DLTDTAARA DTAARA({lib}/COVDA1)",
])

# IN and OUT of *LDA and of every data area with *DTAARA.
member("COV023", [
    "dcl-ds lda dtaara(*lda) qualified;",
    "  part char(10);",
    "  num zoned(5:0);",
    "end-ds;",
], [
    "lda.part = 'local';",
    "lda.num = 123;",
    "out lda;",
    "clear lda;",
    "in *dtaara;",
] + p("'lda [' + lda.part + '] ' + %char(lda.num)"))

# IN of a data area that does not exist; OUT without a lock.
member("COV024", ["dcl-s da char(10) dtaara('COVNODA');"], [
    "in(e) da;",
] + p("'in ' + %char(%status) + ' ' + %error") + guard(["in da;"], "in mon") + guard(["out da;"], "out mon"))

# LEAVESR in free form, in a loop inside the subroutine.
member("COV025", ["dcl-s i int(10);", "dcl-s n int(10);"], [
    "exsr count;",
] + p("'n ' + %char(n)") + ["*inlr = *on;", "begsr count;", "  for i = 1 to 10;", "    n += i;",
                              "    if n > 10;", "      leavesr;", "    endif;", "  endfor;"]
  + p("'not here'", "  ") + ["endsr;"], tail=[])

# LEAVESR outside a subroutine.
member("COV026", [], ["leavesr;"] + p("'x'"))

# SHTDN (fixed form) and %SHTDN.
fixed("COV027", [], [
    C(op="SHTDN", hi="99"),
    *show("'shtdn ' + *IN99 + ' ' + %SHTDN"),
])

# SHTDN without a resulting indicator.
fixed("COV028", [], [C(op="SHTDN"), *show("'x'")])

# WHENxx, ANDxx, ORxx and OTHER (fixed form).
fixed("COV029", [D("N", "S", to="3", typ="P", dec="0"), D("M", "S", to="3", typ="P", dec="0", kw="INZ(5)")], [
    C(f1="1", op="DO", f2="4", res="N"),
    C(op="SELECT"),
    C(f1="N", op="WHENEQ", f2="1"),
    *show("%CHAR(N) + ' one'"),
    C(f1="N", op="WHENEQ", f2="2"),
    C(f1="M", op="ANDGT", f2="4"),
    *show("%CHAR(N) + ' two and m'"),
    C(f1="N", op="WHENEQ", f2="9"),
    C(f1="N", op="OREQ", f2="3"),
    *show("%CHAR(N) + ' three'"),
    C(op="OTHER"),
    *show("%CHAR(N) + ' other'"),
    C(op="ENDSL"),
    C(op="ENDDO"),
])

# WHENxx outside a SELECT group.
fixed("COV030", [D("N", "S", to="3", typ="P", dec="0")], [
    C(f1="N", op="WHENEQ", f2="1"),
    *show("'x'"),
])

# SELECT with an operand: WHEN-IS and WHEN-IN (7.5).
member("COV031", ["dcl-s n int(10);", "dcl-s c char(3) inz('b');"], [
    "for n = 1 to 5;",
    "  select n;",
    "  when-is 1;",
] + p("%char(n) + ' is one'", "    ") + [
    "  when-in %list(2:3);",
] + p("%char(n) + ' in list'", "    ") + [
    "  when-in %range(4:4);",
] + p("%char(n) + ' in range'", "    ") + [
    "  other;",
] + p("%char(n) + ' other'", "    ") + [
    "  endsl;",
    "endfor;",
    "select c;",
    "when-is 'a';",
] + p("'a'", "  ") + ["when-in %list('b':'c');"] + p("'b or c'", "  ") + ["endsl;"])

# WHEN-IS with an operand of another type.
member("COV032", ["dcl-s n int(10) inz(1);"], [
    "select n;",
    "when-is 'one';",
] + p("'one'", "  ") + ["endsl;"])

# XML-INTO a structure from a string.
member("COV033", [
    "dcl-ds info qualified;",
    "  name varchar(20);",
    "  qty int(10);",
    "  price packed(7:2);",
    "end-ds;",
    "dcl-s doc varchar(200);",
], [
    "doc = '<info><name>Bolt</name><qty>12</qty><price>3.50</price></info>';",
    "xml-into info %xml(doc:'doc=string');",
] + p("info.name + ' ' + %char(info.qty) + ' ' + %editc(info.price:'X')") + [
    "doc = '<INFO><NAME>Nut</NAME><QTY>7</QTY><PRICE>0.25</PRICE></INFO>';",
    "xml-into info %xml(doc:'doc=string case=upper');",
] + p("info.name + ' ' + %char(info.qty) + ' ' + %editc(info.price:'X')"))

# XML-INTO an array, a path, and the PSDS element count.
member("COV034", [
    "dcl-ds item qualified dim(5);",
    "  id int(10);",
    "  text varchar(10);",
    "end-ds;",
    "dcl-ds sds psds;",
    "  elems int(20) pos(372);",
    "end-ds;",
    "dcl-s i int(10);",
    "dcl-s doc varchar(300);",
], [
    "doc = '<list><item><id>1</id><text>one</text></item>' + '<item><id>2</id><text>two</text></item>'",
    "    + '<item><id>3</id><text>three</text></item></list>';",
    "xml-into item %xml(doc:'doc=string path=list/item');",
] + p("'elements ' + %char(elems)") + [
    "for i = 1 to elems;",
] + p("%char(item(i).id) + ' ' + item(i).text", "  ") + ["endfor;"])

# XML-INTO options: attributes, allowmissing, allowextra, trim.
member("COV035", [
    "dcl-ds part qualified;",
    "  id int(10);",
    "  name varchar(20);",
    "  note varchar(20) inz('unset');",
    "end-ds;",
], [
    "xml-into part %xml('<part id=\"7\"><name>  wheel  </name><color>red</color></part>'",
    "  :'doc=string allowmissing=yes allowextra=yes');",
] + p("%char(part.id) + ' [' + part.name + '] ' + part.note") + [
    "xml-into part %xml('<part id=\"8\"><name>  axle </name><note>n</note></part>':'doc=string trim=none');",
] + p("%char(part.id) + ' [' + part.name + '] ' + part.note"))

# XML-INTO errors: a missing element, an extra one, bad XML, bad data.
member("COV036", [
    "dcl-ds info qualified;",
    "  name varchar(10);",
    "  qty int(10);",
    "end-ds;",
], guard(["xml-into info %xml('<info><name>x</name></info>':'doc=string');"], "missing")
  + guard(["xml-into info %xml('<info><name>x</name><qty>1</qty><z>1</z></info>':'doc=string');"], "extra")
  + guard(["xml-into info %xml('<info><name>x</name><qty>1</qty>':'doc=string');"], "bad xml")
  + guard(["xml-into info %xml('<info><name>x</name><qty>abc</qty></info>':'doc=string');"], "bad data")
  + guard(["xml-into info %xml('<info><name>x</name><qty>1</qty></info>':'doc=string bogus=1');"], "bad option"))

# XML-INTO with %HANDLER: the elements in groups.
member("COV037", [
    "dcl-ds item qualified template;",
    "  id int(10);",
    "end-ds;",
    "dcl-s total int(10);",
], [
    "xml-into %handler(got:total) %xml('<l><item><id>1</id></item><item><id>2</id></item>' +",
    "  '<item><id>3</id></item></l>':'doc=string path=l/item');",
] + p("'total ' + %char(total)"), tail=[
    "*inlr = *on;",
    "dcl-proc got;",
    "  dcl-pi *n int(10);",
    "    sum int(10);",
    "    items likeds(item) dim(2) const;",
    "    n uns(10) value;",
    "  end-pi;",
    "  dcl-s i int(10);",
    "  for i = 1 to n;",
    "    sum += items(i).id;",
    "  endfor;",
    "  line = 'got ' + %char(n);",
    "  write QSYSPRT line;",
    "  return 0;",
    "end-proc;",
])

SAX_HANDLER = [
    "*inlr = *on;",
    "dcl-proc hdl;",
    "  dcl-pi *n int(10);",
    "    n int(10);",
    "    event int(10) value;",
    "    str pointer value;",
    "    len int(20) value;",
    "    exc int(10) value;",
    "  end-pi;",
    "  dcl-s val char(100) based(str);",
    "  dcl-s nm varchar(12);",
    "  n += 1;",
    "  select;",
    "  when event = *XML_START_DOCUMENT;",
    "    nm = 'startdoc';",
    "  when event = *XML_VERSION_INFO;",
    "    nm = 'version';",
    "  when event = *XML_ENCODING_DECL;",
    "    nm = 'encoding';",
    "  when event = *XML_START_ELEMENT;",
    "    nm = 'start';",
    "  when event = *XML_CHARS;",
    "    nm = 'chars';",
    "  when event = *XML_PREDEF_REF;",
    "    nm = 'predef';",
    "  when event = *XML_END_ELEMENT;",
    "    nm = 'end';",
    "  when event = *XML_ATTR_NAME;",
    "    nm = 'attr';",
    "  when event = *XML_ATTR_CHARS;",
    "    nm = 'attrchars';",
    "  when event = *XML_ATTR_PREDEF_REF;",
    "    nm = 'attrpredef';",
    "  when event = *XML_END_ATTR;",
    "    nm = 'endattr';",
    "  when event = *XML_COMMENT;",
    "    nm = 'comment';",
    "  when event = *XML_START_CDATA;",
    "    nm = 'cdata';",
    "  when event = *XML_END_CDATA;",
    "    nm = 'endcdata';",
    "  when event = *XML_PI_TARGET;",
    "    nm = 'pi';",
    "  when event = *XML_PI_DATA;",
    "    nm = 'pidata';",
    "  when event = *XML_END_DOCUMENT;",
    "    nm = 'enddoc';",
    "  when event = *XML_EXCEPTION;",
    "    nm = 'exception';",
    "  other;",
    "    nm = 'other';",
    "  endsl;",
    "  if len > 0 and len <= 100;",
    "    line = nm + ' ' + %char(len) + ' [' + %subst(val:1:len) + ']';",
    "  else;",
    "    line = nm + ' ' + %char(len);",
    "  endif;",
    "  if event = *XML_EXCEPTION;",
    "    line = %trimr(line) + ' exc ' + %char(exc);",
    "  endif;",
    "  write QSYSPRT line;",
    "  return 0;",
    "end-proc;",
]

# XML-SAX: the events of a small document.
member("COV038", ["dcl-s count int(10);"], [
    "xml-sax %handler(hdl:count) %xml('<?xml version=\"1.0\"?><a x=\"1\"><b>hi</b><c/></a>':'doc=string');",
] + p("'events ' + %char(count)"), tail=SAX_HANDLER)

# XML-SAX: references, a processing instruction and whitespace. (Not
# a comment or CDATA: their '!' and '[' are other characters in
# pub400's CCSID 273.)
member("COV039", ["dcl-s count int(10);"], [
    "xml-sax %handler(hdl:count)",
    "  %xml('<r t=\"a&amp;b\"><?tgt some data?> x &lt; y &apos;</r>':'doc=string');",
] + p("'events ' + %char(count)"), tail=SAX_HANDLER)

# XML-SAX of a document that is not well formed.
member("COV040", ["dcl-s count int(10);"], [
    "monitor;",
    "  xml-sax %handler(hdl:count) %xml('<a><b></a>':'doc=string');",
    "on-error;",
] + p("'sax ' + %char(%status)", "  ") + ["endmon;"] + p("'events ' + %char(count)"), tail=SAX_HANDLER)

# XML-SAX: a handler that stops the parse with a nonzero return.
member("COV041", ["dcl-s count int(10);"], [
    "monitor;",
    "  xml-sax %handler(stop:count) %xml('<a><b>1</b><c>2</c></a>':'doc=string');",
    "on-error;",
] + p("'sax ' + %char(%status)", "  ") + ["endmon;"] + p("'events ' + %char(count)"), tail=[
    "*inlr = *on;",
    "dcl-proc stop;",
    "  dcl-pi *n int(10);",
    "    n int(10);",
    "    event int(10) value;",
    "    str pointer value;",
    "    len int(20) value;",
    "    exc int(10) value;",
    "  end-pi;",
    "  n += 1;",
    "  if n = 3;",
    "    return -1;",
    "  endif;",
    "  return 0;",
    "end-proc;",
])

# ---- Built-in functions ------------------------------------------------

# %FLOAT, %SQRT and float arithmetic, shown through %DEC.
member("COV042", ["dcl-s f float(8);", "dcl-s g float(4);", "dcl-s c char(10) inz(' 2.5 ');"], [
    "f = %float(3) / 4;",
] + p("'a ' + %editc(%dec(f:9:4):'X')") + [
    "f = %float(c) * 2;",
] + p("'b ' + %editc(%dec(f:9:4):'X')") + [
    "g = %sqrt(%float(2));",
] + p("'c ' + %editc(%dech(g:9:4):'X')") + [
    "f = %sqrt(16);",
] + p("'d ' + %editc(%dec(f:9:4):'X') + ' ' + %char(%int(%sqrt(81)))") + [
    "f = %float(1) / 3;",
] + p("'e ' + %editc(%dech(f:15:12):'X')"))

# %FLOAT and %SQRT errors.
member("COV043", ["dcl-s f float(8);", "dcl-s c char(5) inz('abc');", "dcl-s n packed(5:0) inz(-4);"],
       guard(["f = %float(c);"], "float") + guard(["f = %sqrt(n);"], "sqrt"))

# %SQRT of a character value.
member("COV044", ["dcl-s f float(8);"], ["f = %sqrt('16');"] + p("'x'"))

# %EDITFLT.
member("COV045", ["dcl-s f float(8) inz(1.5);", "dcl-s g float(4) inz(-0.125);", "dcl-s h float(8) inz(12345678.9);"],
       p("'[' + %editflt(f) + ']'") + p("'[' + %editflt(g) + ']'") + p("'[' + %editflt(h) + ']'")
       + p("'[' + %editflt(0) + ']' + %char(%len(%editflt(f))) + ' ' + %char(%len(%editflt(g)))"))

# %EDITFLT of a character value.
member("COV046", [], p("%editflt('abc')"))

# %UNS and %UNSH of numbers and character values.
member("COV047", ["dcl-s u uns(10);", "dcl-s p packed(5:2) inz(12.50);", "dcl-s c char(8) inz(' 42.7 ');"], [
    "u = %uns(p);",
] + p("'a ' + %char(u) + ' ' + %char(%unsh(p))") + [
    "u = %uns(c);",
] + p("'b ' + %char(u) + ' ' + %char(%unsh(c))") + p("'c ' + %char(%uns(7.999)) + ' ' + %char(%unsh(7.5)) + ' ' + %char(%unsh(7.49))"))

# %UNS of a negative number and of bad character data.
member("COV048", ["dcl-s u uns(10);", "dcl-s n packed(5:0) inz(-3);", "dcl-s c char(5) inz('1a');"],
       guard(["u = %uns(n);"], "neg") + guard(["u = %uns(c);"], "char") + guard(["u = %unsh(-0.4);"], "unsh"))

# %GRAPH: character to graphic data and back.
member("COV049", ["dcl-s g graph(5);", "dcl-s v vargraph(10);", "dcl-s c char(10);"], [
    "g = %graph('ab');",
    "v = %graph('xyz');",
    "c = %char(v);",
] + p("'graph ' + %char(%len(g)) + ' ' + %char(%size(g)) + ' ' + %char(%len(v)) + ' [' + c + ']'"))

# %GRAPH of a number.
member("COV050", ["dcl-s g graph(5);"], ["g = %graph(12);"] + p("'x'"))

# %NULLIND on fields with NULLIND (ALWNULL(*USRCTL)).
member("COV051", ["dcl-s c char(5) nullind inz('abc');", "dcl-s n packed(5:0) nullind;"], [
    "%nullind(c) = *on;",
] + p("'c ' + %nullind(c) + ' ' + %nullind(n)") + [
    "%nullind(c) = *off;",
    "n = 5;",
] + p("'c ' + %nullind(c) + ' [' + c + '] ' + %char(n)"),
    head=["**FREE", "ctl-opt dftactgrp(*no) alwnull(*usrctl);", "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"])

# %NULLIND of a field that is not null-capable.
member("COV052", ["dcl-s c char(5);"], p("'c ' + %nullind(c)"),
       head=["**FREE", "ctl-opt dftactgrp(*no) alwnull(*usrctl);", "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"])

# %PROC in the main procedure and in subprocedures.
member("COV053", [], p("'main ' + %proc()") + ["callp sub1();", "callp Sub_Two();"], tail=[
    "*inlr = *on;",
    "dcl-proc sub1;",
    "  line = 'sub ' + %proc();",
    "  write QSYSPRT line;",
    "end-proc;",
    "dcl-proc Sub_Two;",
    "  dcl-pi *n extproc(*dclcase);",
    "  end-pi;",
    "  line = 'sub ' + %proc() + ' ' + %char(%len(%proc()));",
    "  write QSYSPRT line;",
    "end-proc;",
])

# %PROC with an operand.
member("COV054", [], p("%proc(1)"))

# %TARGET for SND-MSG: *SELF and *CALLER.
member("COV055", [], [
    "monitor;",
    "  snd-msg *escape %msg('CPF9898':'QCPFMSG':'to self') %target(*self);",
    "on-excp 'CPF9898';",
] + p("'self ' + %char(%status)", "  ") + ["endmon;", "monitor;", "  callp raise();", "on-excp 'CPF9898';"]
  + p("'caller ' + %char(%status)", "  ") + ["endmon;"], tail=[
    "*inlr = *on;",
    "dcl-proc raise;",
    "  snd-msg *escape %msg('CPF9898':'QCPFMSG':'up one') %target(*caller);",
    "end-proc;",
])

# %TARGET outside SND-MSG.
member("COV056", ["dcl-s c char(10);"], ["c = %target(*self);"] + p("c"))

# %THIS outside a Java method.
member("COV057", ["dcl-s o object(*java:'java.lang.Object');"], ["o = %this;"] + p("'x'"))

# %TLOOKUPLT and %TLOOKUPGE over an ascending table and its pair.
member("COV058", [
    "dcl-s tabkey char(1) dim(5) ctdata perrcd(5) ascend;",
    "dcl-s tabval char(5) dim(5) alt(tabkey);",
], [
    "if %tlookuplt('c':tabkey:tabval);",
] + p("'lt ' + tabkey + ' ' + tabval", "  ") + ["endif;", "if %tlookupge('c':tabkey:tabval);"]
  + p("'ge ' + tabkey + ' ' + tabval", "  ") + ["endif;", "if not %tlookuplt('a':tabkey);"]
  + p("'none below a'", "  ") + ["endif;", "if %tlookupge('z':tabkey);"] + p("'z found'", "  ")
  + ["else;"] + p("'nothing at or above z, still ' + tabkey", "  ") + ["endif;"],
    tail=["*inlr = *on;", "**ctdata tabkey", "aone  btwo  dfour fsix  height"])

# %TLOOKUPLT over a table with no sequence.
member("COV059", ["dcl-s tabx char(1) dim(3) ctdata perrcd(3);"], [
    "if %tlookuplt('b':tabx);",
] + p("tabx", "  ") + ["endif;"], tail=["*inlr = *on;", "**ctdata tabx", "ace"])

# %DATA and %PARSER outside DATA-INTO.
member("COV060", ["dcl-s c char(10);"], ["c = %data('x');"] + p("c"))
member("COV061", ["dcl-s c char(10);"], ["c = %parser('X');"] + p("c"))

# %GEN outside DATA-GEN.
member("COV062", ["dcl-s c char(10);"], ["c = %gen('X');"] + p("c"))

# %XML outside XML-INTO; %HANDLER outside XML-SAX.
member("COV063", ["dcl-s c char(10);"], ["c = %xml('<a/>');"] + p("c"))
member("COV064", ["dcl-s c char(10);"], ["c = %handler(x:c);"] + p("c"))

# %HIVAL and %LOVAL.
member("COV065", ["dcl-s p packed(5:2);", "dcl-s i int(5);", "dcl-s u uns(3);", "dcl-s z zoned(3:0);"], [
    "p = %hival(p);",
    "i = %loval(i);",
] + p("'p ' + %editc(p:'X') + ' i ' + %char(i) + ' ' + %char(%hival(i))")
  + p("'u ' + %char(%hival(u)) + ' ' + %char(%loval(u)) + ' z ' + %char(%loval(z))"))

# %HIVAL of a character value.
member("COV066", ["dcl-s c char(3);"], ["c = %hival(c);"] + p("c"))

# %PASSED and %OMITTED.
member("COV067", [], ["callp opt(1);", "callp opt(1:2);", "callp opt(1:*omit);"], tail=[
    "*inlr = *on;",
    "dcl-proc opt;",
    "  dcl-pi *n;",
    "    a int(10) const;",
    "    b int(10) const options(*nopass:*omit);",
    "  end-pi;",
    "  line = 'parms ' + %char(%parms) + ' passed ' + %passed(b) + ' omitted ' + %omitted(b);",
    "  write QSYSPRT line;",
    "end-proc;",
])

# %PASSED of a name that is not a parameter.
member("COV068", ["dcl-s x int(10);"], p("'p ' + %passed(x)"))

# %OMITTED of a parameter without OPTIONS(*OMIT).
member("COV069", [], ["callp opt(1);"], tail=[
    "*inlr = *on;",
    "dcl-proc opt;",
    "  dcl-pi *n;",
    "    a int(10) const;",
    "  end-pi;",
    "  line = 'o ' + %omitted(a);",
    "  write QSYSPRT line;",
    "end-proc;",
])

# %PARMNUM with %PARMS.
member("COV070", [], ["callp three(1:2:3);", "callp three(1);"], tail=[
    "*inlr = *on;",
    "dcl-proc three;",
    "  dcl-pi *n;",
    "    a int(10) const;",
    "    b int(10) const options(*nopass);",
    "    c int(10) const options(*nopass);",
    "  end-pi;",
    "  line = 'parms ' + %char(%parms) + ' b ' + %char(%parmnum(b)) + ' c ' + %char(%parmnum(c));",
    "  if %parms >= %parmnum(c);",
    "    line = %trimr(line) + ' sum ' + %char(a + b + c);",
    "  endif;",
    "  write QSYSPRT line;",
    "end-proc;",
])

# %PARMNUM of a name that is not a parameter.
member("COV071", ["dcl-s x int(10);"], p("%char(%parmnum(x))"))

# ---- Control keywords --------------------------------------------------

CTL = ["**FREE", None, "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"]


def ctl(name, opts, decls, body, tail=None, dirs=()):
    member(name, decls, body, tail=tail, dirs=dirs,
           head=[CTL[0], f"ctl-opt dftactgrp(*no) {opts};", CTL[2], CTL[3]])


# Keywords that change the object, not the run.
ctl("COV072", "copyright('Quartz cover') dftname(COVX) enbpfrcol(*pep)\n  optimize(*basic) prfdta(*nocol)",
    [], p("'object keywords'"))
ctl("COV073", "aut(*use) usrprf(*user) stgmdl(*snglvl) langid(*jobrun) text('cover test')",
    [], p("'more object keywords'"))
ctl("COV074", "actgrp('COVAG') alloc(*teraspace) dclopt(*nochgdslen)",
    ["dcl-s p pointer;", "dcl-s b char(10) based(p);"], [
    "p = %alloc(10);",
    "b = 'teraspace';",
] + p("b") + ["dealloc p;"])

# DATEDIT and the Y edit code.
ctl("COV075", "datedit(*ymd-)", ["dcl-s d zoned(6:0) inz(261024);", "dcl-s e zoned(8:0) inz(20261024);"],
    p("%editc(d:'Y') + ' ' + %editc(e:'Y')"))

# DECEDIT and CURSYM.
ctl("COV076", "decedit('0,') cursym('#')", ["dcl-s n packed(7:2) inz(0.5);", "dcl-s m packed(9:2) inz(-12345.67);"],
    p("'[' + %editc(n:'J') + '] [' + %editc(m:'1':*cursym) + ']'"))

# DECPREC: the precision of %CHAR of an expression.
ctl("COV077", "decprec(30)", ["dcl-s a packed(30:0) inz(999999999999999999999999999999);", "dcl-s b packed(5:2) inz(1.5);"],
    p("%char(%len(%char(a * b)))") + guard(["a = a * b;"], "mult"))

# EXTBININT: binary fields of a file as integers.
ctl("COV078", "extbinint(*yes)", ["dcl-ds rec extname('TYPESF') qualified end-ds;"], [
    "rec.tbin = 2000000000;",
] + p("'size ' + %char(%size(rec.tbin)) + ' ' + %char(rec.tbin) + ' ' + %char(%size(rec.tbin2))"),
    dirs=["//; dds: TYPESF", "//; sqlfile: TYPESF"])

# FIXNBR(*ZONED): a zoned value with blanks is repaired.
ctl("COV079", "fixnbr(*zoned:*inputpacked)", [
    "dcl-ds ds;",
    "  c char(3);",
    "  z zoned(3:0) overlay(c);",
    "end-ds;",
    "dcl-s n packed(5:0);",
], [
    "c = ' 1 ';",
    "n = z + 1;",
] + p("'fixed ' + %char(n)"))

# FLTDIV: division in floating point.
ctl("COV080", "fltdiv(*yes)", ["dcl-s r packed(15:13);", "dcl-s a packed(3:0) inz(2);", "dcl-s b packed(3:0) inz(3);"], [
    "r = a / b;",
] + p("%editc(r:'X')"))

# GENLVL and INTPREC.
ctl("COV081", "genlvl(20) intprec(20)", ["dcl-s a int(10) inz(100000);", "dcl-s b int(10) inz(100000);", "dcl-s c int(20);"], [
    "c = a * b;",
] + p("%char(c)"))

# INTPREC(10): the same product overflows.
ctl("COV082", "intprec(10)", ["dcl-s a int(10) inz(100000);", "dcl-s b int(10) inz(100000);", "dcl-s c int(20);"],
    guard(["c = a * b;"], "mult") + p("%char(c)"))

# INDENT, COPYNEST and SRTSEQ(*HEX).
ctl("COV083", "indent('| ') copynest(5) srtseq(*hex)", ["dcl-s i int(10);"], [
    "for i = 1 to 2;",
    "  if i = 2;",
    "    line = 'indent ' + %char(i);",
    "    write QSYSPRT line;",
    "  endif;",
    "endfor;",
])

# OPENOPT, REQPREXP and TRUNCNBR in fixed form: overflow is an error.
fixed("COV084", [D("N", "S", to="3", typ="P", dec="0", kw="INZ(999)")], [
    C(op="ADD(E)", f2="1", res="N"),
    *show("'add ' + %CHAR(%STATUS) + ' ' + %CHAR(N)"),
    C(op="Z-ADD(E)", f2="1234", res="N"),
    *show("'z-add ' + %CHAR(%STATUS) + ' ' + %CHAR(N)"),
], hspec="DFTACTGRP(*NO) OPENOPT(*INZOFL) REQPREXP(*WARN) TRUNCNBR(*NO)")

# TRUNCNBR(*YES): the same operations truncate.
fixed("COV085", [D("N", "S", to="3", typ="P", dec="0", kw="INZ(999)")], [
    C(op="ADD(E)", f2="1", res="N"),
    *show("'add ' + %CHAR(%STATUS) + ' ' + %CHAR(N)"),
    C(op="Z-ADD(E)", f2="1234", res="N"),
    *show("'z-add ' + %CHAR(%STATUS) + ' ' + %CHAR(N)"),
], hspec="DFTACTGRP(*NO) TRUNCNBR(*YES)")

# THREAD(*CONCURRENT) with a SERIALIZE procedure.
ctl("COV086", "thread(*concurrent)", [], ["callp one();"], tail=[
    "*inlr = *on;",
    "dcl-proc one serialize;",
    "  line = 'serialized';",
    "  write QSYSPRT line;",
    "end-proc;",
])

# SERIALIZE without THREAD(*CONCURRENT).
member("COV087", [], ["callp one();"], tail=[
    "*inlr = *on;",
    "dcl-proc one serialize;",
    "  line = 'x';",
    "  write QSYSPRT line;",
    "end-proc;",
])

# VALIDATE, CVTOPT and CCSIDCVT.
ctl("COV088", "validate(*nodatetime) cvtopt(*nodatetime) ccsidcvt(*excp)", ["dcl-s d date inz(d'2026-10-24');"],
    p("%char(d:*iso)"))

# CCSID(*CHAR) and the CCSID keyword of a field.
ctl("COV089", "ccsid(*char:37)", ["dcl-s c char(10) inz('abc');", "dcl-s u varchar(10) ccsid(*utf8);",
                                  "dcl-s h char(10) ccsid(*hex);"], [
    "u = c;",
    "h = c;",
    "c = u;",
] + p("'[' + c + '] ' + %char(%len(u)) + ' ' + %char(%size(u))"))

# PGMINFO for the program and a procedure; REQPROTO and RTNPARM.
ctl("COV090", "pgminfo(*pcml:*module)", [], p("'v ' + get()"), tail=[
    "*inlr = *on;",
    "dcl-proc get pgminfo(*no) reqproto(*no);",
    "  dcl-pi *n varchar(20) rtnparm;",
    "  end-pi;",
    "  return 'rtnparm ' + %char(%parms);",
    "end-proc;",
])

# FORMSALIGN, FTRANS(*NONE) and ALTSEQ(*NONE).
ctl("COV091", "formsalign(*yes) ftrans(*none) altseq(*none)", [], p("'forms'"))

# ALTSEQ(*SRC): 'a' collates after 'z'; a field exempt with ALTSEQ(*NONE).
M["COV092"] = [
    H("DFTACTGRP(*NO) ALTSEQ(*SRC)"),
    PRT_F,
    PRT_D,
    D("A", "S", to="1", kw="INZ('a')"),
    D("Z", "S", to="1", kw="INZ('z')"),
    D("X", "S", to="1", kw="INZ('a') ALTSEQ(*NONE)"),
    C(f1="A", op="IFGT", f2="Z"),
    *show("'a after z'"),
    C(op="ELSE"),
    *show("'a before z'"),
    C(op="ENDIF"),
    C(f1="X", op="IFGT", f2="Z"),
    *show("'x after z'"),
    C(op="ELSE"),
    *show("'x before z'"),
    C(op="ENDIF"),
    CX("EVAL", "*INLR = *ON"),
    "**ALTSEQ",
    "ALTSEQ    81AA",
]

# SRTSEQ(*LANGIDSHR) with LANGID: case shares a weight.
ctl("COV093", "srtseq(*langidshr) langid('ENU')", ["dcl-s a char(1) inz('a');", "dcl-s b char(1) inz('A');",
                                                  "dcl-s c char(1) inz('B');", "dcl-s r1 ind;", "dcl-s r2 ind;", "dcl-s r3 ind;"],
    ["r1 = (a = b);", "r2 = (a < c);", "r3 = (c > a);"] + p("'eq ' + r1 + ' lt ' + r2 + ' gt ' + r3"))

# BNDDIR with a service program the setup binds.
ctl("COV094", "bnddir('COVBD')", ["dcl-pr dbl int(10);", "  n int(10) value;", "end-pr;"],
    p("'bnddir ' + %char(dbl(21))"), dirs=[
        "//; with: COV094S",
        "//; setup: CRTRPGMOD MODULE({lib}/COV094S) SRCFILE({lib}/QRPGLESRC) SRCMBR(COV094S) OUTPUT(*NONE)",
        "//; setup: CRTSRVPGM SRVPGM({lib}/COV094S) MODULE({lib}/COV094S) EXPORT(*ALL)",
        "//; setup: CRTBNDDIR BNDDIR({lib}/COVBD)",
        "//; setup: ADDBNDDIRE BNDDIR({lib}/COVBD) OBJ(({lib}/COV094S *SRVPGM))",
        "//; cleanup: DLTBNDDIR BNDDIR({lib}/COVBD)",
        "//; cleanup: DLTSRVPGM SRVPGM({lib}/COV094S)",
        "//; cleanup: DLTMOD MODULE({lib}/COV094S)",
    ])
INC["COV094S"] = [
    "**FREE",
    "ctl-opt nomain;",
    "dcl-proc dbl export;",
    "  dcl-pi *n int(10);",
    "    n int(10) value;",
    "  end-pi;",
    "  return n * 2;",
    "end-proc;",
]

# ---- File keywords -----------------------------------------------------

# EXTIND: a file conditioned by U1, which is off.
member("COV095", ["dcl-f QPRINT printer(132) extind(*inu1);"], [
    "write QPRINT line;",
] + p("'u1 ' + *inu1 + ' open ' + %open(QPRINT)"))

# FORMLEN and FORMOFL with an overflow indicator.
member("COV096", ["dcl-s i int(10);", "dcl-s ovf ind;"], [
    "for i = 1 to 14;",
    "  if ovf;",
] + p("'heading'", "    ") + ["    ovf = *off;", "  endif;"] + p("'line ' + %char(i)", "  ") + ["endfor;"],
    head=["**FREE", "ctl-opt dftactgrp(*no);", "dcl-f QSYSPRT printer(132) formlen(10) formofl(8) oflind(ovf);",
          "dcl-ds line len(132) end-ds;"])

# PRTCTL: space and skip from a structure, and the line count.
M["COV097"] = [
    H("DFTACTGRP(*NO)"),
    F("QSYSPRT", "O", fmt="F", rlen="132", dev="PRINTER", kw="PRTCTL(PC)"),
    D("PC", "DS"),
    D(" SPB", frm="1", to="3"),
    D(" SPA", frm="4", to="6"),
    D(" SKB", frm="7", to="9"),
    D(" SKA", frm="10", to="12"),
    D(" CUR", frm="13", to="15", typ="S", dec="0"),
    D("TXT", "S", to="20"),
    D("SAVE", "S", to="3", typ="S", dec="0"),
    CX("EVAL", "SPB = '  1'"),
    CX("EVAL", "TXT = 'first'"),
    C(op="EXCEPT", f2="ONE"),
    CX("EVAL", "SAVE = CUR"),
    CX("EVAL", "SPB = '  2'"),
    CX("EVAL", "TXT = 'after two'"),
    C(op="EXCEPT", f2="ONE"),
    CX("EVAL", "SPB = ' '"),
    CX("EVAL", "SKB = ' 10'"),
    CX("EVAL", "TXT = 'at ten ' + %CHAR(SAVE) + ' ' + %CHAR(CUR)"),
    C(op="EXCEPT", f2="ONE"),
    CX("EVAL", "*INLR = *ON"),
    O_rec("QSYSPRT", "E", exc="ONE"),
    O_fld(field="TXT", end="20"),
]

# KEYLOC: a program-described keyed file, its key in the record.
member("COV098", [
    "dcl-f CUSTF disk(40) keyed(*char:3) keyloc(1);",
    "dcl-ds rec len(40);",
    "  name char(20) pos(4);",
    "end-ds;",
    "dcl-ds key;",
    "  k packed(5:0);",
    "end-ds;",
], guard(["k = 104;", "chain key CUSTF rec;"] + p("'chain ' + %found(CUSTF) + ' ' + name"), "keyloc")
  + guard(["k = 999;", "chain key CUSTF rec;"] + p("'chain ' + %found(CUSTF)"), "keyloc 999"),
    dirs=["//; dds: CUSTF", "//; sqlfile: CUSTF"])

# PGMNAME: a SPECIAL file whose records a program gives.
member("COV099", [
    "dcl-f SPCL special(20) usage(*input) pgmname('COV099P');",
    "dcl-ds rec len(20) end-ds;",
], [
    "read SPCL rec;",
    "dow not %eof(SPCL);",
] + p("'got ' + rec", "  ") + ["  read SPCL rec;", "enddo;"] + p("'eof'"), dirs=[
    "//; with: COV099P",
    "//; setup: CRTBNDRPG PGM({lib}/COV099P) SRCFILE({lib}/QRPGLESRC) SRCMBR(COV099P) OUTPUT(*NONE)",
    "//; cleanup: DLTPGM PGM({lib}/COV099P)",
])
INC["COV099P"] = [
    "**FREE",
    "ctl-opt dftactgrp(*no) actgrp(*caller);",
    "dcl-pi *n;",
    "  opt char(1);",
    "  sts char(1);",
    "  err zoned(5:0);",
    "  area char(20);",
    "end-pi;",
    "dcl-s n int(10);",
    "select;",
    "when opt = 'O';",
    "  n = 0;",
    "  sts = '0';",
    "when opt = 'R';",
    "  n += 1;",
    "  if n > 3;",
    "    sts = '1';",
    "  else;",
    "    area = 'record ' + %char(n);",
    "    sts = '0';",
    "  endif;",
    "when opt = 'C';",
    "  sts = '0';",
    "  *inlr = *on;",
    "other;",
    "  sts = '0';",
    "endsl;",
    "return;",
]

# PGMNAME naming a program that does not exist.
member("COV100", [
    "dcl-f SPCL special(20) usage(*input) pgmname('COVNOSPCL') usropn;",
    "dcl-ds rec len(20) end-ds;",
], ["open(e) SPCL;"] + p("'open ' + %char(%status(SPCL)) + ' ' + %error"))

# SAVEDS, SAVEIND, MAXDEV and SLN on a workstation file (compiled only).
member("COV101", [
    "dcl-f B8A workstn maxdev(*file) saveds(keep) saveind(10) sln(line1) usropn;",
    "dcl-ds keep;",
    "  a char(5);",
    "end-ds;",
    "dcl-s line1 zoned(2:0) inz(5);",
], ["a = 'x';"] + p("'saved ' + a"), dirs=["//; dds: B8A", "//; run: no"])

# HANDLER: an Open Access handler that does not exist.
member("COV102", [
    "dcl-f COVOA disk(20) usage(*input) handler('COVNOHDL') usropn;",
    "dcl-ds rec len(20) end-ds;",
], ["open(e) COVOA;"] + p("'open ' + %char(%status(COVOA)) + ' ' + %error"))

# RAFDATA on a file that is not a record address file.
member("COV103", [
    "dcl-f CUSTF disk usage(*input) rafdata(CUSTF);",
], ["read CUSTF;"] + p("'x'"), dirs=["//; dds: CUSTF", "//; sqlfile: CUSTF"])

# ALIAS: a file's alternative names in a LIKEREC structure.
member("COV104", [
    "dcl-f COVALI disk usage(*input) alias qualified;",
    "dcl-ds r likerec(COVALI.ALIR);",
], [
    "read COVALI r;",
    "dow not %eof(COVALI);",
] + p("%char(r.item_number) + ' ' + r.item_description", "  ") + ["  read COVALI r;", "enddo;"],
    dirs=["//; dds: COVALI", "//; sql: INSERT INTO {lib}.COVALI VALUES (1, 'first'), (2, 'second')"])

# ALIAS on a definition: EXTNAME with the alternative names.
member("COV105", ["dcl-ds r extname('COVALI') alias qualified end-ds;"], [
    "r.item_number = 9;",
    "r.item_description = 'aliased';",
] + p("%char(r.item_number) + ' ' + r.item_description"),
    dirs=["//; dds: COVALI", "//; sql: INSERT INTO {lib}.COVALI VALUES (1, 'first')"])

# EXTMBR, INCLUDE, RECNO and the record number in the INFDS.
member("COV106", [
    "dcl-f CUSTF disk usage(*input) extmbr('CUSTF') include(CUSTR) recno(rrn) infds(fb);",
    "dcl-s rrn packed(10:0);",
    "dcl-ds fb;",
    "  frrn int(10) pos(397);",
    "end-ds;",
], [
    "read CUSTF;",
    "read CUSTF;",
] + p("%char(CUSNO) + ' ' + %char(rrn) + ' ' + %char(frrn)") + [
    "chain 5 CUSTF;",
] + p("'chain ' + %char(CUSNO) + ' ' + %found(CUSTF)"), dirs=["//; dds: CUSTF", "//; sqlfile: CUSTF"])

# IGNORE of a file's only format.
member("COV107", ["dcl-f CUSTF disk usage(*input) ignore(CUSTR);"], ["read CUSTF;"] + p("'x'"),
       dirs=["//; dds: CUSTF", "//; sqlfile: CUSTF"])

# LIKEFILE: a second file like the first.
member("COV108", [
    "dcl-f CUSTF keyed usage(*input) block(*no);",
    "dcl-f CUST2 likefile(CUSTF) extfile('CUSTF') usropn;",
    "dcl-ds r likerec(CUSTR);",
], [
    "open CUST2;",
    "chain (103) CUST2 r;",
] + p("'cust2 ' + r.name") + ["chain (105) CUSTF r;"] + p("'custf ' + r.name") + ["close CUST2;"],
    dirs=["//; dds: CUSTF", "//; sqlfile: CUSTF"])

# LIKEFILE on a parameter: a file passed to a procedure.
member("COV129", [
    "dcl-f CUSTF keyed usage(*input) block(*no);",
], ["callp show(CUSTF);"], tail=[
    "*inlr = *on;",
    "dcl-proc show;",
    "  dcl-pi *n;",
    "    f likefile(CUSTF);",
    "  end-pi;",
    "  dcl-ds r2 likerec(CUSTR);",
    "  chain (105) f r2;",
    "  line = 'param ' + r2.name;",
    "  write QSYSPRT line;",
    "end-proc;",
], dirs=["//; dds: CUSTF", "//; sqlfile: CUSTF"])

# ---- Definition keywords -----------------------------------------------

# ALIGN, PACKEVEN and SAMEPOS.
M["COV109"] = [
    H("DFTACTGRP(*NO)"),
    PRT_F,
    PRT_D,
    D("AL", "DS", kw="ALIGN"),
    D(" C1", to="1"),
    D(" I4", to="10", typ="I", dec="0"),
    D(" C2", to="1"),
    D(" I8", to="20", typ="I", dec="0"),
    D("PE", "DS"),
    D(" PK", frm="1", to="2", typ="P", dec="0", kw="PACKEVEN"),
    D(" PO", frm="3", to="4", typ="P", dec="0"),
    D("SP", "DS", kw="QUALIFIED"),
    D(" A", to="4"),
    D(" B", to="6"),
    D(" C", to="4", kw="SAMEPOS(B)"),
    CX("EVAL", "PK = 99"),
    CX("EVAL", "PO = 999"),
    CX("EVAL", "SP.B = 'abcdef'"),
    *show("%CHAR(%SIZE(AL)) + ' ' + %CHAR(%LEN(PK)) + ' ' + %CHAR(%LEN(PO)) + ' ' + %CHAR(PK) + ' ' + SP.C"),
    CX("EVAL", "*INLR = *ON"),
]

# PACKEVEN on a field that is not packed.
M["COV110"] = [
    H("DFTACTGRP(*NO)"),
    PRT_F,
    PRT_D,
    D("PE", "DS"),
    D(" Z", frm="1", to="2", typ="S", dec="0", kw="PACKEVEN"),
    CX("EVAL", "*INLR = *ON"),
]

# CLASS: a Java object reference, declared but not used.
M["COV111"] = [
    H("DFTACTGRP(*NO) THREAD(*SERIALIZE)"),
    PRT_F,
    PRT_D,
    D("STR", "S", typ="O", kw="CLASS(*JAVA:'java.lang.String')"),
    CX("IF", "STR = *NULL"),
    *show("'class null'"),
    C(op="ENDIF"),
    CX("EVAL", "*INLR = *ON"),
]

# EXTFLD and EXTFMT on an externally described structure.
member("COV112", [
    "dcl-ds c extname('CUSTF') qualified;",
    "  who extfld('NAME');",
    "  bal extfld extfmt('S');",
    "end-ds;",
], [
    "c.who = 'renamed';",
    "c.bal = -1.25;",
] + p("c.who + ' ' + %editc(c.bal:'X') + ' ' + %char(%size(c.bal)) + ' ' + %char(%size(c))"),
    dirs=["//; dds: CUSTF", "//; sqlfile: CUSTF"])

# EXTFLD naming a field the file does not have.
member("COV113", ["dcl-ds c extname('CUSTF') qualified;", "  who extfld('NOFIELD');", "end-ds;"],
       p("c.name"), dirs=["//; dds: CUSTF", "//; sqlfile: CUSTF"])

# TOFILE: a compile-time array printed when the program ends.
M["COV114"] = [
    H("DFTACTGRP(*NO)"),
    F("QSYSPRT", "O", fmt="F", rlen="132", dev="PRINTER"),
    D("ARR", "S", to="10", kw="DIM(3) CTDATA PERRCD(1)"),
    D(kw="TOFILE(QSYSPRT)"),
    CX("EVAL", "ARR(2) = 'changed'"),
    CX("EVAL", "*INLR = *ON"),
    "**CTDATA ARR",
    "first",
    "second",
    "third",
]

# FROMFILE: a pre-run-time array read from a file.
M["COV115"] = [
    "      *; dds: MSTF",
    "      *; sqlfile: MSTF",
    H("DFTACTGRP(*NO)"),
    F("MSTF", "I", desig="T", fmt="F", rlen="13", dev="DISK"),
    PRT_F,
    PRT_D,
    D("ARR", "S", to="13", kw="DIM(4) FROMFILE(MSTF) PERRCD(1)"),
    D("I", "S", to="10", typ="I", dec="0"),
    C(f1="1", op="DO", f2="4", res="I"),
    *show("'arr ' + ARR(I)"),
    C(op="ENDDO"),
    CX("EVAL", "*INLR = *ON"),
]

# NOOPT, OPDESC, RTNPARM and %PARMS with RTNPARM.
member("COV116", [
    "dcl-s n packed(5:0) noopt inz(5);",
    "dcl-pr desc varchar(20) opdesc rtnparm;",
    "  c char(10) const options(*varsize);",
    "end-pr;",
], [
    "n += 1;",
] + p("'n ' + %char(n) + ' ' + desc('abc')"), tail=[
    "*inlr = *on;",
    "dcl-proc desc;",
    "  dcl-pi *n varchar(20) opdesc rtnparm;",
    "    c char(10) const options(*varsize);",
    "  end-pi;",
    "  return 'parms ' + %char(%parms);",
    "end-proc;",
])

# DTAARA(*AUTO): read and written around the program without IN/OUT.
member("COV117", [
    "dcl-ds da dtaara(*auto:'COVDA2') len(20);",
    "end-ds;",
], p("'auto [' + da + ']'") + ["da = 'updated';"], dirs=[
    "//; setup: CRTDTAARA DTAARA({lib}/COVDA2) TYPE(*CHAR) LEN(20) VALUE('auto value')",
    "//; cleanup: DLTDTAARA DTAARA({lib}/COVDA2)",
])

# The NULLIND keyword of a structure subfield and a file's null field.
member("COV118", [
    "dcl-f COVNUL disk usage(*input);",
], [
    "read COVNUL;",
    "dow not %eof(COVNUL);",
] + p("%char(NKEY) + ' ' + %nullind(NVAL) + ' [' + NVAL + ']'", "  ") + ["  read COVNUL;", "enddo;"],
    head=["**FREE", "ctl-opt dftactgrp(*no) alwnull(*usrctl);", "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"],
    dirs=["//; dds: COVNUL", "//; sql: INSERT INTO {lib}.COVNUL VALUES (1, 'one'), (2, NULL), (3, 'three')"])

# A null-capable field with ALWNULL(*INPUTONLY): read as a value.
member("COV119", ["dcl-f COVNUL disk usage(*input);"], [
    "read COVNUL;",
    "read COVNUL;",
] + p("%char(NKEY) + ' [' + NVAL + ']'"),
    head=["**FREE", "ctl-opt dftactgrp(*no) alwnull(*inputonly);", "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"],
    dirs=["//; dds: COVNUL", "//; sql: INSERT INTO {lib}.COVNUL VALUES (1, 'one'), (2, NULL), (3, 'three')"])

# A data structure DTAARA with IN *LOCK and UNLOCK.
member("COV120", [
    "dcl-ds da dtaara('COVDA3') qualified;",
    "  a char(5);",
    "  b char(5);",
    "end-ds;",
], [
    "in *lock da;",
] + p("'[' + da.a + '] [' + da.b + ']'") + ["unlock da;"], dirs=[
    "//; setup: CRTDTAARA DTAARA({lib}/COVDA3) TYPE(*CHAR) LEN(10) VALUE('abcdeFGHIJ')",
    "//; cleanup: DLTDTAARA DTAARA({lib}/COVDA3)",
])

# DSPLY with a response in a batch job: the inquiry's default reply.
member("COV121", ["dcl-s r char(10) inz('before');", "dcl-s n packed(3:0) inz(5);"], [
    "dsply 'Reply please' '' r;",
] + p("'reply [' + r + ']'") + ["dsply 'Number' '*EXT' n;"] + p("'number ' + %char(n)"))

# XML-INTO with case=any and a varying target.
member("COV122", ["dcl-s v varchar(20);", "dcl-ds pt qualified;", "  xval int(10);", "  yval int(10);", "end-ds;"], [
    "xml-into v %xml('<v>plain text</v>':'doc=string');",
    "xml-into pt %xml('<Pt><XVal>3</XVal><yVAL>-4</yVAL></Pt>':'doc=string case=any');",
] + p("'[' + v + '] ' + %char(pt.xval) + ' ' + %char(pt.yval)"))

# XML-INTO errors at run time: missing and extra elements, bad XML,
# bad data. (COV036 adds an option the compiler refuses.)
member("COV123", [
    "dcl-ds info qualified;",
    "  name varchar(10);",
    "  qty int(10);",
    "end-ds;",
], guard(["xml-into info %xml('<info><name>x</name></info>':'doc=string');"], "missing")
  + guard(["xml-into info %xml('<info><name>x</name><qty>1</qty><z>1</z></info>':'doc=string');"], "extra")
  + guard(["xml-into info %xml('<info><name>x</name><qty>1</qty>':'doc=string');"], "bad xml")
  + guard(["xml-into info %xml('<info><name>x</name><qty>abc</qty></info>':'doc=string');"], "bad data")
  + guard(["xml-into info %xml('<other><name>x</name><qty>1</qty></other>':'doc=string');"], "root"))

# CALLB in a program for a named activation group: by name and by
# pointer.
fixed("COV124", [
    D("N", "S", to="10", typ="I", dec="0", kw="INZ(41)"),
    D("PP", "S", typ="*", kw="PROCPTR INZ(%PADDR('TWICE'))"),
], [
    C(op="CALLB", f2="'ADDONE'"),
    C(op="PARM", res="N"),
    *show("'addone ' + %CHAR(N)"),
    C(op="CALLB", f2="PP"),
    C(op="PARM", res="N"),
    *show("'twice ' + %CHAR(N)"),
], tail=[
    P("ADDONE", "B"),
    D("ADDONE", "PI"),
    D(" X", to="10", typ="I", dec="0"),
    CX("EVAL", "X = X + 1"),
    P("ADDONE", "E"),
    P("TWICE", "B"),
    D("TWICE", "PI"),
    D(" X", to="10", typ="I", dec="0"),
    CX("EVAL", "X = X * 2"),
    P("TWICE", "E"),
], hspec="DFTACTGRP(*NO)")

# CALLB of a procedure nothing exports: the program is not bound.
fixed("COV125", [D("N", "S", to="10", typ="I", dec="0")], [
    C(op="CALLB", f2="'COVNOPROC'"),
    C(op="PARM", res="N"),
    *show("'called'"),
], hspec="DFTACTGRP(*NO)")

# ACQ, REL and NEXT of a workstation file that cannot be opened in a
# batch job.
member("COV126", ["dcl-f B8A workstn usropn maxdev(*file) devid(dev);", "dcl-s dev char(10) inz('none');"], [
    "open(e) B8A;",
] + p("'open ' + %char(%status(B8A)) + ' ' + %error") + [
    "acq(e) 'COVNODEV' B8A;",
] + p("'acq ' + %char(%status(B8A)) + ' ' + %error + ' ' + dev") + [
    "next(e) 'COVNODEV' B8A;",
] + p("'next ' + %char(%status(B8A)) + ' ' + %error") + [
    "rel(e) 'COVNODEV' B8A;",
] + p("'rel ' + %char(%status(B8A)) + ' ' + %error"),
    dirs=["//; dds: B8A", "//; sql: CREATE TABLE {lib}.COVT126 (A CHAR(1) CCSID 37)"])

# TRUNCNBR(*NO) and (*YES) with a monitor: the fixed-form arithmetic
# that overflows is an error, or drops its high digits.
for name, opt in (("COV127", "*NO"), ("COV128", "*YES")):
    fixed(name, [D("N", "S", to="3", typ="P", dec="0", kw="INZ(999)")], [
        CX("MONITOR"),
        C(op="ADD", f2="1", res="N"),
        *show("'add ' + %CHAR(N)"),
        CX("ON-ERROR"),
        *show("'add error ' + %CHAR(%STATUS) + ' ' + %CHAR(N)"),
        CX("ENDMON"),
        CX("MONITOR"),
        C(op="Z-ADD", f2="1234", res="N"),
        *show("'z-add ' + %CHAR(N)"),
        CX("ON-ERROR"),
        *show("'z-add error ' + %CHAR(%STATUS) + ' ' + %CHAR(N)"),
        CX("ENDMON"),
    ], hspec=f"DFTACTGRP(*NO) TRUNCNBR({opt})")

# CALLB of exported procedures of the module, by name and by pointer.
# (Without EXPORT the binder finds neither: COV124.)
fixed("COV130", [
    D("N", "S", to="10", typ="I", dec="0", kw="INZ(41)"),
    D("PP", "S", typ="*", kw="PROCPTR INZ(%PADDR('TWICE'))"),
], [
    C(op="CALLB", f2="'ADDONE'"),
    C(op="PARM", res="N"),
    *show("'addone ' + %CHAR(N)"),
    C(op="CALLB", f2="PP"),
    C(op="PARM", res="N"),
    *show("'twice ' + %CHAR(N)"),
], tail=[
    P("ADDONE", "B", kw="EXPORT"),
    D("ADDONE", "PI"),
    D(" X", to="10", typ="I", dec="0"),
    CX("EVAL", "X = X + 1"),
    P("ADDONE", "E"),
    P("TWICE", "B", kw="EXPORT"),
    D("TWICE", "PI"),
    D(" X", to="10", typ="I", dec="0"),
    CX("EVAL", "X = X * 2"),
    P("TWICE", "E"),
], hspec="DFTACTGRP(*NO)")

# %GRAPH with a graphic CCSID in effect.
ctl("COV131", "ccsid(*graph:835)", ["dcl-s c char(4) inz('ab');", "dcl-s g graph(4);"], [
    "g = %graph(c);",
] + p("'graph ' + %char(%len(g)) + ' ' + %char(%size(g))"))

# OPENOPT and REQPREXP.
ctl("COV132", "openopt(*inzofl) reqprexp(*warn)", [], p("'openopt reqprexp'"))

# IGNORE and INCLUDE of a display file's formats (compiled only).
member("COV133", ["dcl-f B8B workstn ignore(FTR);", "dcl-f B8J workstn include(FTR) usropn;"],
       ["if 1 = 0;", "  open B8J;", "  exfmt HDR;", "  write FTR;", "endif;"] + p("'x'"),
       dirs=["//; dds: B8B", "//; dds: B8J", "//; run: no"])

# SAVEDS and SAVEIND on a multiple-device file (compiled only).
member("COV134", [
    "dcl-f B8A workstn maxdev(*file) saveds(keep) saveind(10) usropn;",
    "dcl-ds keep;",
    "  a char(5);",
    "end-ds;",
], ["if 1 = 0;", "  open B8A;", "endif;", "a = 'x';"] + p("'saved ' + a"), dirs=["//; dds: B8A", "//; run: no"])

for name, lines in M.items():
    with open(os.path.join(OUT, name + ".rpgle"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
for name, lines in INC.items():
    with open(os.path.join(OUT, name + ".rpginc"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
print(len(M), "members")
