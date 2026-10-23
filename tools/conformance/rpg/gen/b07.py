"""Corpus batch 7: database file I/O, free form and fixed form. Writes
../b07io/*.rpgle; the files come from ../dds (b07dds.py).

usage: python b07.py
"""
import os

from fixed import C, CX, D, F, I_fld, I_rec, O_fld, O_rec, PRT_D, PRT_F, comment, place

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b07io")


def head(*files):
    """The free-form head: directives for the files, then the print idiom."""
    out = ["**FREE"]
    for f in files:
        out.append(f"//; dds: {f}")
        if f in SQLS:
            out.append(f"//; sqlfile: {f}")
    out += ["ctl-opt dftactgrp(*no);"]
    return out


PRT = ["dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"]
SQLS = {"CUSTF", "ORDF", "HISTF", "TYPESF", "SALESF", "TRANF", "MSTF", "ACTF"}
CUSLINE = "line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);"
P = "write QSYSPRT line;"

M = {}

M["IO001"] = head("CUSTF") + [
    "dcl-f CUSTF keyed;",
] + PRT + [
    "// Sequential reads by file and by format; %EOF both ways.",
    "read CUSTF;",
    "dow not %eof(CUSTF);",
    CUSLINE, P,
    "read CUSTR;",
    "enddo;",
    "line = 'eof=' + %eof + ' ' + %eof(CUSTF);",
    P,
    "*inlr = *on;",
]

M["IO002"] = head("CUSTF") + [
    "dcl-f CUSTF keyed;",
] + PRT + [
    "dcl-s k packed(5:0) inz(104);",
    "chain k CUSTF;",
    "line = 'found=' + %found(CUSTF) + ' ' + NAME + %char(BAL);",
    P,
    "chain (999) CUSTR;",
    "line = 'found=' + %found + ' ' + NAME + %char(CUSNO);",
    P,
    "chain (101) CUSTF;",
    "if %found(CUSTF);",
    CUSLINE, P,
    "endif;",
    "k = 108;",
    "chain (k) CUSTR;",
    CUSLINE, P,
    "line = 'eof=' + %eof(CUSTF);",
    P,
    "*inlr = *on;",
]

M["IO031"] = head("CUSTF") + [
    "dcl-f CUSTF keyed;",
] + PRT + [
    "dcl-s k packed(5:0) inz(104);",
    "chain k CUSTF;",
    "line = 'found=' + %found(CUSTF) + ' ' + NAME + %char(BAL);",
    P,
    "chain 999 CUSTR;",
    "line = 'found=' + %found + ' ' + NAME + %char(CUSNO);",
    P,
    "chain (101) CUSTF;",
    "if %found(CUSTF);",
    CUSLINE, P,
    "endif;",
    "k = 108;",
    "chain (k) CUSTR;",
    CUSLINE, P,
    "line = 'eof=' + %eof(CUSTF);",
    P,
    "*inlr = *on;",
]

M["IO003"] = head("ORDF") + [
    "dcl-f ORDF keyed;",
] + PRT + [
    "dcl-s c packed(5:0) inz(101);",
    "dcl-ds kds likerec(ORDR:*KEY);",
    "// A partial key, then the full key, then %KDS.",
    "setll c ORDF;",
    "reade c ORDF;",
    "dow not %eof(ORDF);",
    "line = %char(ORDCUS) + '/' + %char(ORDNO) + ' ' + ITEM + %char(QTY) + ' ' + %char(PRICE);",
    P,
    "reade c ORDR;",
    "enddo;",
    "setll (104:2) ORDR;",
    "reade (104) ORDF;",
    "line = 'next ' + %char(ORDCUS) + '/' + %char(ORDNO) + ' eof=' + %eof(ORDF);",
    P,
    "reade (104) ORDF;",
    "line = 'then eof=' + %eof(ORDF);",
    P,
    "kds.ORDCUS = 107;",
    "kds.ORDNO = 2;",
    "chain %kds(kds) ORDF;",
    "line = 'kds ' + ITEM + %char(QTY) + ' ' + %found(ORDF);",
    P,
    "setll %kds(kds:1) ORDF;",
    "read ORDF;",
    "line = 'kds1 ' + %char(ORDCUS) + '/' + %char(ORDNO) + ' ' + ITEM;",
    P,
    "*inlr = *on;",
]

M["IO004"] = head("CUSTF") + [
    "dcl-f CUSTF keyed;",
] + PRT + [
    "// Positioning: *START, *END, *LOVAL, *HIVAL, SETGT; READP.",
    "setll *end CUSTF;",
    "readp CUSTF;",
    "line = 'last ' + %char(CUSNO) + ' ' + %eof(CUSTF);",
    P,
    "setll *start CUSTF;",
    "read CUSTF;",
    "line = 'first ' + %char(CUSNO);",
    P,
    "setgt *hival CUSTF;",
    "readp CUSTF;",
    "line = 'hival ' + %char(CUSNO);",
    P,
    "setll *loval CUSTF;",
    "read CUSTF;",
    "line = 'loval ' + %char(CUSNO);",
    P,
    "setgt (104) CUSTF;",
    "read CUSTF;",
    "line = 'gt 104 read ' + %char(CUSNO);",
    P,
    "setgt (104) CUSTF;",
    "readp CUSTF;",
    "line = 'gt 104 readp ' + %char(CUSNO);",
    P,
    "setll (103) CUSTF;",
    "readp CUSTF;",
    "dow not %eof(CUSTF);",
    "line = 'back ' + %char(CUSNO);",
    P,
    "readp CUSTF;",
    "enddo;",
    "line = 'bof ' + %eof(CUSTF);",
    P,
    "*inlr = *on;",
]

M["IO005"] = head("CUSTF") + [
    "dcl-f CUSTF keyed;",
] + PRT + [
    "// SETLL and SETGT set %FOUND and %EQUAL.",
    "setll (104) CUSTF;",
    "line = 'll 104 f=' + %found(CUSTF) + ' e=' + %equal(CUSTF) + ' ' + %found + %equal;",
    P,
    "setll (100) CUSTF;",
    "line = 'll 100 f=' + %found(CUSTF) + ' e=' + %equal(CUSTF);",
    P,
    "setll (109) CUSTF;",
    "line = 'll 109 f=' + %found(CUSTF) + ' e=' + %equal(CUSTF);",
    P,
    "setgt (107) CUSTF;",
    "line = 'gt 107 f=' + %found(CUSTF);",
    P,
    "setgt (108) CUSTF;",
    "line = 'gt 108 f=' + %found(CUSTF);",
    P,
    "if %equal(CUSTF);",
    "line = 'still equal';",
    "else;",
    "line = 'not equal';",
    "endif;",
    P,
    "*inlr = *on;",
]

M["IO006"] = head("CUSTF") + [
    "dcl-f CUSTF keyed usage(*update:*output);",
] + PRT + [
    "// WRITE new records, then read them all.",
    "CUSNO = 110;",
    "NAME = 'Ivy Irwin';",
    "CITY = 'Austin';",
    "BAL = 10.10;",
    "write CUSTR;",
    "CUSNO = 109;",
    "NAME = 'Jack Jones';",
    "BAL = -9.99;",
    "write CUSTR;",
    "setll *start CUSTF;",
    "read CUSTF;",
    "dow not %eof(CUSTF);",
    CUSLINE, P,
    "read CUSTF;",
    "enddo;",
    "*inlr = *on;",
]

M["IO007"] = head("CUSTF") + [
    "dcl-f CUSTF keyed usage(*update);",
] + PRT + [
    "// UPDATE every field, then only %FIELDS.",
    "chain (102) CUSTF;",
    "BAL = BAL + 100;",
    "NAME = 'Robert Brown';",
    "update CUSTR;",
    "chain (103) CUSTF;",
    "BAL = 33.33;",
    "CITY = 'Nowhere';",
    "update CUSTR %fields(BAL);",
    "chain (105) CUSTR;",
    "NAME = 'Evelyn Evans';",
    "update CUSTR %fields(NAME:CITY);",
    "read CUSTF;",
    "setll *loval CUSTF;",
    "read(n) CUSTF;",
    "dow not %eof(CUSTF);",
    CUSLINE, P,
    "read(n) CUSTF;",
    "enddo;",
    "*inlr = *on;",
]

M["IO008"] = head("CUSTF") + [
    "dcl-f CUSTF keyed usage(*delete);",
] + PRT + [
    "// DELETE after a read and by key; %FOUND.",
    "chain (103) CUSTF;",
    "delete CUSTR;",
    "delete (106) CUSTF;",
    "line = 'del 106 found=' + %found(CUSTF);",
    P,
    "delete (999) CUSTF;",
    "line = 'del 999 found=' + %found(CUSTF);",
    P,
    "setll *start CUSTF;",
    "read(n) CUSTF;",
    "dow not %eof(CUSTF);",
    CUSLINE, P,
    "read(n) CUSTF;",
    "enddo;",
    "*inlr = *on;",
]

M["IO009"] = head("CUSTF", "ORDF") + [
    "dcl-f CUSTF keyed prefix(C_) rename(CUSTR:CUSREC);",
    "dcl-f ORDF keyed prefix('O':1);",
] + PRT + [
    "// PREFIX and RENAME.",
    "chain (107) CUSREC;",
    "line = %char(C_CUSNO) + ' ' + C_NAME + C_CITY;",
    P,
    "setll (C_CUSNO) ORDF;",
    "reade (C_CUSNO) ORDR;",
    "dow not %eof(ORDF);",
    "line = %char(ORDCUS) + ' ' + %char(ORDNO) + ' ' + OTEM + %char(OTY) + ' ' + %char(ORICE);",
    P,
    "reade (C_CUSNO) ORDR;",
    "enddo;",
    "*inlr = *on;",
]

M["IO010"] = head("CUSTF") + [
    "dcl-f CUSTF keyed qualified;",
] + PRT + [
    "dcl-ds cus likerec(CUSTF.CUSTR);",
    "dcl-ds cin likerec(CUSTF.CUSTR:*INPUT);",
    "// A qualified file: I/O through data structures.",
    "read CUSTF cus;",
    "line = %char(cus.CUSNO) + ' ' + cus.NAME + cus.CITY;",
    P,
    "chain (105) CUSTF.CUSTR cin;",
    "line = %char(cin.CUSNO) + ' ' + cin.NAME + %char(cin.BAL);",
    P,
    "read CUSTF.CUSTR cus;",
    "line = %char(cus.CUSNO) + ' ' + cus.NAME + %char(%size(cus));",
    P,
    "*inlr = *on;",
]

M["IO011"] = head("CUSTF", "ORDF") + [
    "dcl-f ORDF keyed;",
] + PRT + [
    "dcl-ds all extname('CUSTF') end-ds;",
    "dcl-ds keys extname('CUSTF':*KEY) qualified end-ds;",
    "dcl-ds ords extname('ORDF':'ORDR':*INPUT) prefix(X_) end-ds;",
    "// EXTNAME data structures: *ALL, *KEY, a format and *INPUT.",
    "CUSNO = 101;",
    "NAME = 'Nobody';",
    "line = %char(%size(all)) + ' ' + %char(%size(keys)) + ' ' + %char(%size(ords));",
    P,
    "line = %subst(all:4:6) + '|' + %char(CUSNO);",
    P,
    "keys.CUSNO = 104;",
    "chain %kds(keys) ORDF;",
    "line = 'ord ' + ITEM + %char(QTY);",
    P,
    "read ORDF ords;",
    "line = 'ds ' + X_ITEM + %char(X_QTY) + ' ' + %char(X_ORDCUS);",
    P,
    "*inlr = *on;",
]

M["IO012"] = head("CUSTF") + [
    "dcl-f CUSTF keyed usropn;",
] + PRT + [
    "// USROPN: OPEN, CLOSE and %OPEN.",
    "line = 'open=' + %open(CUSTF);",
    P,
    "open CUSTF;",
    "line = 'open=' + %open(CUSTF);",
    P,
    "read CUSTF;",
    "read CUSTF;",
    CUSLINE, P,
    "close CUSTF;",
    "line = 'open=' + %open(CUSTF);",
    P,
    "open CUSTF;",
    "read CUSTF;",
    "line = 'again ' + %char(CUSNO);",
    P,
    "close *all;",
    "open QSYSPRT;",
    "line = 'open=' + %open(CUSTF);",
    P,
    "*inlr = *on;",
]

M["IO013"] = head("CUSTF") + [
    "dcl-f CUSTF keyed usage(*update:*output) usropn;",
] + PRT + [
    "// (E): %ERROR and %STATUS after file errors.",
    "read(e) CUSTF;",
    "line = 'closed ' + %error + ' ' + %char(%status) + ' ' + %char(%status(CUSTF));",
    P,
    "open CUSTF;",
    "update(e) CUSTR;",
    "line = 'no read ' + %error + ' ' + %char(%status(CUSTF));",
    P,
    "CUSNO = 101;",
    "write(e) CUSTR;",
    "line = 'dup ' + %error + ' ' + %char(%status(CUSTF));",
    P,
    "chain(e) (101) CUSTF;",
    "line = 'chain ' + %error + ' ' + %char(%status(CUSTF)) + ' ' + %found(CUSTF);",
    P,
    "chain(e) (555) CUSTF;",
    "line = 'nf ' + %error + ' ' + %char(%status(CUSTF)) + ' ' + %found(CUSTF);",
    P,
    "open(e) CUSTF;",
    "line = 'reopen ' + %error + ' ' + %char(%status(CUSTF));",
    P,
    "close CUSTF;",
    "*inlr = *on;",
]

M["IO014"] = head("CUSTF") + [
    "dcl-f CUSTF keyed infds(fb);",
] + PRT + [
    "dcl-ds fb;",
    "  fbfile *file;",
    "  fbopen char(1) pos(9);",
    "  fbeof char(1) pos(10);",
    "  fbstat *status;",
    "  fbop *opcode;",
    "  fbrec *record;",
    "  fbmsg char(7) pos(46);",
    "  fbrrn int(10) pos(397);",
    "end-ds;",
    "// The file information data structure.",
    "read CUSTF;",
    "line = fbfile + '|' + fbopen + fbeof + '|' + %char(fbstat) + '|' + fbop + '|' + fbrec;",
    P,
    "line = 'rrn ' + %char(fbrrn) + ' msg ' + fbmsg + '|';",
    P,
    "chain (106) CUSTR;",
    "line = fbop + '|' + fbrec + '|' + %char(fbstat) + ' rrn ' + %char(fbrrn);",
    P,
    "chain (199) CUSTF;",
    "line = fbop + '|' + %char(fbstat) + '|' + fbeof;",
    P,
    "setgt *hival CUSTF;",
    "read CUSTF;",
    "line = fbop + '|' + %char(fbstat) + '|' + fbeof;",
    P,
    "*inlr = *on;",
]

M["IO019"] = head("CUSTF") + [
    "dcl-f CUSTF keyed usage(*update);",
] + PRT + [
    "// (N): reads without a lock; UNLOCK; an update needs a locked record.",
    "chain(n) (101) CUSTF;",
    "update(e) CUSTR;",
    "line = 'nolock update ' + %char(%status(CUSTF));",
    P,
    "chain (101) CUSTF;",
    "unlock CUSTF;",
    "update(e) CUSTR;",
    "line = 'unlocked update ' + %char(%status(CUSTF));",
    P,
    "chain (101) CUSTF;",
    "BAL = 1;",
    "update CUSTR;",
    "chain(n) (101) CUSTF;",
    CUSLINE, P,
    "*inlr = *on;",
]

M["IO020"] = head("CUSTF") + [
    "dcl-f CUSTF keyed usage(*input:*output);",
] + PRT + [
    "// FEOD, and reads after it.",
    "read CUSTF;",
    "read CUSTF;",
    "feod CUSTF;",
    "line = 'feod ' + %char(CUSNO) + ' ' + %error;",
    P,
    "read CUSTF;",
    "line = 'after ' + %char(CUSNO) + ' ' + %eof(CUSTF);",
    P,
    "*inlr = *on;",
]

M["IO021"] = head("CUSTF") + [
    "dcl-f CUSTF keyed usage(*update:*output);",
] + PRT + [
    "dcl-ds rec likerec(CUSTR:*ALL);",
    "dcl-ds inp likerec(CUSTR:*INPUT);",
    "// Data structure I/O: read into a structure, write and update from one.",
    "read CUSTF inp;",
    "line = 'ds ' + %char(inp.CUSNO) + ' ' + inp.NAME + ' fld ' + %char(CUSNO) + '|';",
    P,
    "rec.CUSNO = 120;",
    "rec.NAME = 'From DS';",
    "rec.CITY = 'Austin';",
    "rec.BAL = 1.5;",
    "write CUSTR rec;",
    "chain (120) CUSTF rec;",
    "rec.BAL = 2.5;",
    "update CUSTR rec;",
    "chain(n) (120) CUSTF;",
    CUSLINE, P,
    "*inlr = *on;",
]

M["IO022"] = head("HISTF") + [
    "dcl-f HISTF infds(fb);",
] + PRT + [
    "dcl-ds fb;",
    "  rrn int(10) pos(397);",
    "end-ds;",
    "// Arrival sequence: CHAIN and SETLL by relative record number.",
    "chain 3 HISTF;",
    "line = %char(HCUS) + ' ' + HTEXT + %char(HAMT) + ' rrn ' + %char(rrn);",
    P,
    "chain 9 HISTF;",
    "line = 'chain 9 ' + %found(HISTF);",
    P,
    "setll 4 HISTF;",
    "read HISTF;",
    "line = %char(HCUS) + ' ' + HTEXT + ' rrn ' + %char(rrn);",
    P,
    "setll *start HISTF;",
    "read HISTF;",
    "dow not %eof(HISTF);",
    "line = %char(rrn) + ': ' + %char(HCUS) + ' ' + HTEXT + %char(HAMT);",
    P,
    "read HISTF;",
    "enddo;",
    "*inlr = *on;",
]

M["IO023"] = head("CUSTF", "CUSTCITY") + [
    "dcl-f CUSTCITY keyed;",
] + PRT + [
    "// A logical file keyed by city and number.",
    "read CUSTCITY;",
    "dow not %eof(CUSTCITY);",
    CUSLINE, P,
    "read CUSTCITY;",
    "enddo;",
    "setll ('Chicago') CUSTCITY;",
    "reade ('Chicago') CUSTR;",
    "dow not %eof;",
    "line = 'chi ' + %char(CUSNO) + ' ' + NAME;",
    P,
    "reade ('Chicago') CUSTR;",
    "enddo;",
    "chain ('Denver':106) CUSTCITY;",
    "line = 'chain ' + %found + ' ' + NAME;",
    P,
    "*inlr = *on;",
]

M["IO025"] = head("TYPESF") + [
    "dcl-f TYPESF keyed;",
] + PRT + [
    "// Fields of each type from an external description.",
    "read TYPESF;",
    "dow not %eof(TYPESF);",
    "line = %char(TKEY) + ' ' + %char(TZON) + ' ' + %char(TBIN) + ' ' + %char(TBIN2);",
    P,
    "line = %char(TDATE) + ' ' + %char(TTIME) + ' ' + %char(TSTMP);",
    P,
    "line = '[' + TVAR + '] ' + %char(%len(TVAR));",
    P,
    "read TYPESF;",
    "enddo;",
    "line = %char(%size(TZON)) + ' ' + %char(%size(TBIN)) + ' ' + %char(%size(TBIN2));",
    P,
    "*inlr = *on;",
]

M["IO026"] = head("TYPESF") + [
    "dcl-f TYPESF keyed usage(*update:*output);",
] + PRT + [
    "// Writing and updating fields of each type.",
    "TKEY = 9;",
    "TZON = -1.5;",
    "TBIN = 77;",
    "TBIN2 = 3.25;",
    "TDATE = d'2024-02-29';",
    "TTIME = t'08.30.00';",
    "TSTMP = z'2024-02-29-08.30.00.000500';",
    "TVAR = 'written';",
    "write TYPR;",
    "chain (2) TYPESF;",
    "TDATE = d'2001-01-01';",
    "TVAR = %trim(TVAR) + '!';",
    "update TYPR;",
    "setll *start TYPESF;",
    "read(n) TYPESF;",
    "dow not %eof(TYPESF);",
    "line = %char(TKEY) + ' ' + %char(TZON) + ' ' + %char(TBIN) + ' ' + %char(TBIN2);",
    P,
    "line = '  ' + %char(TDATE) + ' ' + %char(TTIME) + ' ' + %char(TSTMP) + ' [' + TVAR + ']';",
    P,
    "read(n) TYPESF;",
    "enddo;",
    "*inlr = *on;",
]

M["IO028"] = head("ORDF") + [
    "dcl-f ORDF keyed;",
] + PRT + [
    "// READPE backwards over one key; READE and READPE without a key.",
    "setgt (101) ORDF;",
    "readpe (101) ORDF;",
    "dow not %eof(ORDF);",
    "line = 'pe ' + %char(ORDCUS) + '/' + %char(ORDNO);",
    P,
    "readpe (101) ORDF;",
    "enddo;",
    "setll (107) ORDF;",
    "read ORDF;",
    "reade *key ORDF;",
    "line = 'key ' + %char(ORDCUS) + '/' + %char(ORDNO) + ' ' + %eof(ORDF);",
    P,
    "reade *key ORDF;",
    "line = 'key eof ' + %eof(ORDF);",
    P,
    "*inlr = *on;",
]

M["IO029"] = head("CUSTF", "ORDF") + [
    "dcl-f CUSTF keyed;",
    "dcl-f ORDF keyed;",
] + PRT + [
    "dcl-s total packed(9:2);",
    "// A join by hand: each customer and the value of its orders.",
    "read CUSTF;",
    "dow not %eof(CUSTF);",
    "total = 0;",
    "setll (CUSNO) ORDF;",
    "reade (CUSNO) ORDF;",
    "dow not %eof(ORDF);",
    "total += QTY * PRICE;",
    "reade (CUSNO) ORDF;",
    "enddo;",
    "line = %char(CUSNO) + ' ' + NAME + %editc(total:'J');",
    P,
    "read CUSTF;",
    "enddo;",
    "*inlr = *on;",
]

M["IO030"] = head("CUSTF", "ORDF") + [
    "dcl-f CUSTF keyed usropn;",
    "dcl-f ORDF keyed usropn;",
] + PRT + [
    "// OPEN and CLOSE several; *ALL; a subprocedure reads a global file.",
    "dcl-pr count int(10);",
    "end-pr;",
    "open CUSTF;",
    "open ORDF;",
    "line = 'orders ' + %char(count()) + ' ' + %open(CUSTF) + %open(ORDF);",
    P,
    "close *all;",
    "open QSYSPRT;",
    "line = 'closed ' + %open(CUSTF) + %open(ORDF);",
    P,
    "*inlr = *on;",
    "dcl-proc count;",
    "  dcl-pi *n int(10);",
    "  end-pi;",
    "  dcl-s n int(10);",
    "  read ORDF;",
    "  dow not %eof(ORDF);",
    "    n += 1;",
    "    read ORDF;",
    "  enddo;",
    "  return n;",
    "end-proc;",
]


# Fixed form.
def fixed_head(*files):
    out = []
    for f in files:
        out.append(place([(7, "*"), (8, f"; dds: {f}")]))
        if f in SQLS:
            out.append(place([(7, "*"), (8, f"; sqlfile: {f}")]))
    return out


def show(expr):
    return [CX("EVAL", "LINE = " + expr), C(op="WRITE", f2="QSYSPRT", res="LINE")]


M["IO015"] = fixed_head("CUSTF") + [
    comment("A program-described input file: I specs over CUSTF."),
    F("CUSTF", "I", "F", fmt="F", rlen="40", dev="DISK"),
    PRT_F,
    PRT_D,
    I_rec("CUSTF", "NS"),
    I_fld("CNO", 1, 3, 0, fmt="P"),
    I_fld("CNAME", 4, 23),
    I_fld("CCITY", 24, 35),
    I_fld("CBAL", 36, 40, 2, fmt="P"),
    I_fld("CFIRST", 4, 4),
    C(op="READ", f2="CUSTF", eq="99"),
    C(f1="*IN99", op="DOWEQ", f2="*OFF"),
] + show("%CHAR(CNO) + ' ' + CNAME + CCITY + %CHAR(CBAL) + ' ' + CFIRST") + [
    C(op="READ", f2="CUSTF", eq="99"),
    C(op="ENDDO"),
    C(op="SETON", hi="LR"),
]

M["IO016"] = fixed_head("HISTF") + [
    comment("Program-described output to a disk file: O specs with ADD."),
    F("HISTF", "O", fmt="F", rlen="22", dev="DISK", kw="BLOCK(*NO)"),
    F("HISTIN", "I", "F", fmt="E", dev="DISK", kw="EXTFILE('HISTF') EXTDESC('HISTF')"),
    place([(6, "F"), (44, "RENAME(HISTR:HIN) PREFIX(I)")]),
    PRT_F,
    PRT_D,
    D("NUM", "S", to="5", typ="P", dec="0"),
    D("TXT", "S", to="15"),
    D("AMT", "S", to="7", typ="P", dec="2"),
    CX("EVAL", "NUM = 201"),
    CX("EVAL", "TXT = 'Added one'"),
    CX("EVAL", "AMT = 12.5"),
    C(op="EXCEPT", f2="NEWREC"),
    CX("EVAL", "NUM = 202"),
    CX("EVAL", "AMT = -1"),
    C(op="EXCEPT", f2="NEWREC"),
    C(op="READ", f2="HISTIN", eq="99"),
    C(f1="*IN99", op="DOWEQ", f2="*OFF"),
] + show("%CHAR(IHCUS) + ' ' + IHTEXT + %CHAR(IHAMT)") + [
    C(op="READ", f2="HISTIN", eq="99"),
    C(op="ENDDO"),
    C(op="SETON", hi="LR"),
    O_rec("HISTF", "E", exc="NEWREC"),
    O_fld("NUM", 3, fmt="P"),
    O_fld("TXT", 18),
    O_fld("AMT", 22, fmt="P"),
]

M["IO017"] = fixed_head("ORDF") + [
    comment("Fixed form: KLIST and KFLD, CHAIN and READE with indicators."),
    F("ORDF", "I", "F", fmt="E", rtype="K", dev="DISK"),
    PRT_F,
    PRT_D,
    D("KCUS", "S", to="5", typ="P", dec="0"),
    D("KORD", "S", to="5", typ="P", dec="0"),
    C(f1="ORDKEY", op="KLIST"),
    C(op="KFLD", res="KCUS"),
    C(op="KFLD", res="KORD"),
    C(f1="PARTKY", op="KLIST"),
    C(op="KFLD", res="KCUS"),
    CX("EVAL", "KCUS = 101"),
    CX("EVAL", "KORD = 2"),
    C(f1="ORDKEY", op="CHAIN", f2="ORDF", hi="50"),
] + show("'chain ' + *IN50 + ' ' + ITEM") + [
    CX("EVAL", "KORD = 9"),
    C(f1="ORDKEY", op="CHAIN", f2="ORDR", hi="50"),
] + show("'chain ' + *IN50 + ' ' + ITEM") + [
    CX("EVAL", "KCUS = 104"),
    C(f1="PARTKY", op="SETLL", f2="ORDF", eq="51"),
    C(f1="PARTKY", op="READE", f2="ORDF", eq="52"),
    C(f1="*IN52", op="DOWEQ", f2="*OFF"),
] + show("%CHAR(ORDNO) + ' ' + ITEM + ' ' + *IN51") + [
    C(f1="PARTKY", op="READE", f2="ORDF", eq="52"),
    C(op="ENDDO"),
    C(op="SETON", hi="LR"),
]

M["IO018"] = fixed_head("CUSTF") + [
    comment("Fixed form: SETLL and SETGT indicators, READP, CHAIN (N)."),
    F("CUSTF", "U", "F", fmt="E", rtype="K", dev="DISK"),
    PRT_F,
    PRT_D,
    D("KEY", "S", to="5", typ="P", dec="0"),
    C(op="Z-ADD", f2="104", res="KEY"),
    C(f1="KEY", op="SETLL", f2="CUSTF", hi="60", eq="61"),
] + show("'ll 104 ' + *IN60 + *IN61") + [
    C(op="Z-ADD", f2="109", res="KEY"),
    C(f1="KEY", op="SETLL", f2="CUSTF", hi="60", eq="61"),
] + show("'ll 109 ' + *IN60 + *IN61") + [
    C(op="Z-ADD", f2="103", res="KEY"),
    C(f1="KEY", op="SETGT", f2="CUSTR", hi="62"),
    C(op="READ", f2="CUSTF", eq="63"),
] + show("'gt 103 ' + *IN62 + ' ' + %CHAR(CUSNO) + *IN63") + [
    C(op="READP", f2="CUSTF", eq="63"),
] + show("'readp ' + %CHAR(CUSNO) + *IN63") + [
    C(op="Z-ADD", f2="105", res="KEY"),
    C(f1="KEY", op="CHAIN(N)", f2="CUSTF", hi="64"),
] + show("'chain ' + %CHAR(CUSNO) + ' ' + NAME + *IN64") + [
    C(f1="KEY", op="CHAIN", f2="CUSTF", hi="64"),
    CX("EVAL", "BAL = BAL * 2"),
    C(op="UPDATE", f2="CUSTR", lo="65"),
    C(f1="KEY", op="CHAIN(N)", f2="CUSTF", hi="64"),
] + show("'upd ' + %CHAR(BAL) + ' ' + *IN65") + [
    C(op="UPDATE", f2="CUSTR", lo="65"),
] + show("'upd again ' + *IN65 + ' ' + %CHAR(%STATUS(CUSTF))") + [
    C(op="SETON", hi="LR"),
]

M["IO024"] = fixed_head("CUSTF") + [
    comment("Fixed form: WRITE and DELETE with indicators, an INFDS."),
    F("CUSTF", "U", "F", add="A", fmt="E", rtype="K", dev="DISK", kw="INFDS(FB)"),
    PRT_F,
    PRT_D,
    D("KEY", "S", to="5", typ="P", dec="0"),
    D("FB", "DS"),
    D("FSTAT", frm="*STATUS"),
    D("FOP", frm="*OPCODE"),
    CX("EVAL", "CUSNO = 102"),
    C(op="WRITE", f2="CUSTR", lo="70"),
] + show("'dup ' + *IN70 + ' ' + %CHAR(FSTAT) + ' ' + FOP") + [
    CX("EVAL", "CUSNO = 130"),
    CX("EVAL", "NAME = 'New One'"),
    C(op="WRITE", f2="CUSTR", lo="70"),
] + show("'new ' + *IN70 + ' ' + %CHAR(FSTAT) + ' ' + FOP") + [
    C(op="Z-ADD", f2="130", res="KEY"),
    C(f1="KEY", op="DELETE", f2="CUSTR", hi="71", lo="72"),
] + show("'del ' + *IN71 + *IN72 + ' ' + FOP") + [
    C(op="Z-ADD", f2="131", res="KEY"),
    C(f1="KEY", op="DELETE", f2="CUSTF", hi="71", lo="72"),
] + show("'del ' + *IN71 + *IN72 + ' ' + FOP") + [
    C(f1="*LOVAL", op="SETLL", f2="CUSTF"),
    C(op="READ", f2="CUSTF", eq="99"),
    C(f1="*IN99", op="DOWEQ", f2="*OFF"),
] + show("%CHAR(CUSNO) + ' ' + NAME") + [
    C(op="READ", f2="CUSTF", eq="99"),
    C(op="ENDDO"),
    C(op="SETON", hi="LR"),
]

M["IO027"] = fixed_head("CUSTF") + [
    comment("Fixed form: I specs renaming fields of an external file."),
    F("CUSTF", "I", "F", fmt="E", rtype="K", dev="DISK"),
    PRT_F,
    PRT_D,
    place([(6, "I"), (7, "CUSTR"), (21, "01")]),
    place([(6, "I"), (21, "CUSNO"), (49, "NUMBER")]),
    place([(6, "I"), (21, "BAL"), (49, "AMOUNT"), (69, "11"), (71, "12"), (73, "13")]),
    C(op="READ", f2="CUSTF", eq="99"),
    C(f1="*IN99", op="DOWEQ", f2="*OFF"),
] + show("%CHAR(NUMBER) + ' ' + NAME + %CHAR(AMOUNT) + ' ' + *IN01 + *IN11 + *IN12 + *IN13") + [
    C(op="READ", f2="CUSTF", eq="99"),
    C(op="ENDDO"),
    C(op="SETON", hi="LR"),
]


def wrap_free(lines):
    for l in "\n".join(lines).split("\n"):
        if len(l) > 100:
            raise SystemExit(f"too long: {l}")
    return lines


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    for name, lines in M.items():
        with open(os.path.join(OUT, name + ".rpgle"), "w", newline="\n") as f:
            f.write("\n".join(wrap_free(lines)) + "\n")
    print(len(M), "members")
