"""Corpus batch 8: display files in RPG. Writes ../b08dsp/*.rpgle; the
display files come from ../dds (b08dds.py). Each member is called at a
display: its `keys:` lines answer the screens it reads, one each.

usage: python b08.py
"""
import os

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b08dsp")


def head(files, keys, extra=()):
    """The free-form head: the files, the operator's answers."""
    out = ["**FREE"]
    for f in files:
        out.append(f"//; dds: {f}")
        if f == "CUSTF":
            out.append("//; sqlfile: CUSTF")
    out += list(extra)
    out += [f"//; keys: {k}" for k in keys]
    out += ["ctl-opt dftactgrp(*no);"]
    return out


M = {}

M["DSP001"] = head(["B8A"], ["3,10=ALICE 4,10=42 enter", "f3"]) + [
    "dcl-f B8A workstn;",
    "// EXFMT with input and output fields; CA03 ends.",
    "MSG = 'Type a name';",
    "dou *in03;",
    "exfmt SCR1;",
    "if not *in03;",
    "MSG = 'Hello ' + %trim(NAME) + ' ' + %char(CNT * 2);",
    "endif;",
    "enddo;",
    "*inlr = *on;",
]

M["DSP002"] = head(["B8B"], ["5,8=AB12 6,8=7 enter", "f5", "5,8=ZZ enter", "f3"]) + [
    "dcl-f B8B workstn;",
    "dcl-s n int(10);",
    "// A header, a footer and a query record over them (OVERLAY).",
    "HTXT = 'Header written once';",
    "write HDR;",
    "RESULT = 'Nothing yet';",
    "dou *in03;",
    "write FTR;",
    "exfmt QRY;",
    "n += 1;",
    "if *in05;",
    "RESULT = 'Refresh ' + %char(n);",
    "elseif not *in03;",
    "RESULT = 'Code ' + CODE + ' qty ' + %char(QTY) + ' n ' + %char(n);",
    "endif;",
    "enddo;",
    "*inlr = *on;",
]

M["DSP003"] = head(["B8C"], ["enter", "enter", "5,10=X enter", "enter", "f3"]) + [
    "dcl-f B8C workstn;",
    "// Option indicators on constants, a field and an attribute.",
    "dou *in03;",
    "COUNT += 1;",
    "*in40 = %rem(COUNT: 2) = 0;",
    "*in41 = COUNT >= 3;",
    "*in42 = COUNT = 2;",
    "*in43 = COUNT >= 2;",
    "*in44 = COUNT >= 4;",
    "STAT = 'Pass ' + %char(COUNT) + ' opt ' + OPT;",
    "exfmt SCR;",
    "enddo;",
    "*inlr = *on;",
]

M["DSP004"] = head(["B8D"], ["3,10=ABC f5", "3,10=XYZ f12", "3,10=QQ f10", "enter", "f3"]) + [
    "dcl-f B8D workstn;",
    "// CF keys bring the data back, CA keys do not; *INKx follow the key.",
    "RES = 'Start';",
    "dou *in03;",
    "exfmt KEYS;",
    "RES = 'k05=' + *in05 + ' k10=' + *in10 + ' k12=' + *in12 + ' val=' + VAL;",
    "RES2 = 'ke=' + *inke + ' kj=' + *inkj + ' kl=' + *inkl + ' kc=' + *inkc;",
    "enddo;",
    "*inlr = *on;",
]

M["DSP005"] = head(["B8E"], ["3,10=150 enter", "reset 3,10=50 enter", "reset 4,10=PAID enter", "f3"]) + [
    "dcl-f B8E workstn;",
    "// ERRMSG on a field until the input is right.",
    "dou not *in50 and not *in51;",
    "exfmt ENTRY;",
    "if *in03;",
    "leave;",
    "endif;",
    "*in50 = AMT > 99;",
    "*in51 = NOTE = *blanks;",
    "enddo;",
    "DONE = 'Accepted ' + %char(AMT) + ' ' + NOTE;",
    "exfmt ENTRY;",
    "*inlr = *on;",
]

M["DSP006"] = head(
    ["B8F"],
    ["3,10=X enter", "reset 3,10=OK enter", "f3"],
    [
        "//; setup: CRTMSGF MSGF({lib}/B8MSGF)",
        "//; setup: ADDMSGD MSGID(USR8001) MSGF({lib}/B8MSGF) MSG('The code must be OK')",
        "//; cleanup: DLTMSGF MSGF({lib}/B8MSGF)",
    ],
) + [
    "dcl-f B8F workstn;",
    "// ERRMSGID: the message from a message file.",
    "dou not *in52;",
    "exfmt ENTRY;",
    "*in52 = CODE <> 'OK';",
    "enddo;",
    "DONE = 'Code accepted';",
    "exfmt ENTRY;",
    "*inlr = *on;",
]

M["DSP007"] = head(["B8D"], ["@3,12 enter", "@5,20 f5", "@1,1 f10", "f3"]) + [
    "dcl-f B8D workstn infds(fb);",
    "dcl-ds fb;",
    "  fbfmt char(8) pos(38);",
    "  aid uns(3) pos(369);",
    "  crow uns(3) pos(370);",
    "  ccol uns(3) pos(371);",
    "end-ds;",
    "// The display feedback: format, attention identifier, cursor.",
    "RES = 'Start';",
    "dou *in03;",
    "exfmt KEYS;",
    "RES = 'fmt=' + %trim(fbfmt) + ' aid=' + %char(aid) + ' row=' + %char(crow) + ' col=' + %char(ccol);",
    "enddo;",
    "*inlr = *on;",
]

M["DSP008"] = head(["B8H"], ["4,10=ONE enter", "4,10=TWO f6", "enter", "f3"]) + [
    "dcl-f B8H workstn indds(inds);",
    "dcl-ds inds len(99);",
    "  exit ind pos(3);",
    "  six ind pos(6);",
    "  thirty ind pos(30);",
    "  showit ind pos(31);",
    "end-ds;",
    "dcl-s n int(10);",
    "// INDARA with an indicator structure; *IN stays as it was.",
    "*in30 = *on;",
    "dou exit;",
    "n += 1;",
    "thirty = %rem(n: 2) = 0;",
    "showit = n >= 3;",
    "RES = 'n=' + %char(n) + ' six=' + six + ' in30=' + *in30 + ' in06=' + *in06 + ' inp=' + INP;",
    "exfmt SCR;",
    "enddo;",
    "*inlr = *on;",
]

M["DSP009"] = head(["B8H"], ["4,10=AAA enter", "f6", "f3"]) + [
    "dcl-f B8H workstn;",
    "dcl-s n int(10);",
    "// INDARA without a structure: the area is *IN01-*IN99.",
    "dou *in03;",
    "n += 1;",
    "*in30 = n = 2;",
    "*in31 = n = 3;",
    "RES = 'n=' + %char(n) + ' in06=' + *in06 + ' inp=' + INP;",
    "exfmt SCR;",
    "enddo;",
    "*inlr = *on;",
]

M["DSP010"] = head(["B8J"], ["pgdn", "pgdn", "pgup", "f3"]) + [
    "dcl-f B8J workstn sfile(SFL1: rrn);",
    "dcl-s rrn packed(4: 0);",
    "dcl-s i int(10);",
    "// A subfile loaded whole; the system rolls its pages.",
    "*in33 = *on;",
    "write CTL1;",
    "*in33 = *off;",
    "for i = 1 to 10;",
    "SNUM = i * 3;",
    "SNAM = 'Entry number ' + %char(i);",
    "rrn = i;",
    "write SFL1;",
    "endfor;",
    "*in31 = *on;",
    "*in32 = *on;",
    "*in34 = *on;",
    "dou *in03;",
    "write FTR;",
    "exfmt CTL1;",
    "enddo;",
    "*inlr = *on;",
]

M["DSP011"] = head(["B8K"], ["6,2:1 8,2:4 enter", "7,2:1 enter", "enter", "f3"]) + [
    "dcl-f B8K workstn sfile(SFL2: rrn);",
    "dcl-s rrn packed(4: 0);",
    "dcl-s i int(10);",
    "dcl-s n int(10);",
    "// Options typed in a subfile, read back with READC and updated.",
    "*in33 = *on;",
    "write CTL2;",
    "*in33 = *off;",
    "for i = 1 to 5;",
    "ITEM = 'ITEM' + %char(i);",
    "DESC = 'Description ' + %char(i);",
    "OPT = ' ';",
    "rrn = i;",
    "write SFL2;",
    "endfor;",
    "*in31 = *on;",
    "*in34 = *on;",
    "MSG = 'Type options';",
    "dou *in03;",
    "write FTR2;",
    "exfmt CTL2;",
    "if *in03;",
    "leave;",
    "endif;",
    "n = 0;",
    "readc SFL2;",
    "dow not %eof(B8K);",
    "n += 1;",
    "select;",
    "when OPT = '1';",
    "DESC = 'Marked at ' + %char(rrn);",
    "when OPT = '4';",
    "DESC = 'Dropped at ' + %char(rrn);",
    "endsl;",
    "OPT = ' ';",
    "update SFL2;",
    "readc SFL2;",
    "enddo;",
    "MSG = %char(n) + ' changed';",
    "enddo;",
    "*inlr = *on;",
]

M["DSP012"] = head(["B8K"], ["6,2:1 enter", "enter", "f3"]) + [
    "dcl-f B8K workstn sfile(SFL2: rrn);",
    "dcl-s rrn packed(4: 0);",
    "dcl-s i int(10);",
    "dcl-s n int(10);",
    "// SFLNXTCHG: an updated record is read by READC again.",
    "*in33 = *on;",
    "write CTL2;",
    "*in33 = *off;",
    "for i = 1 to 3;",
    "ITEM = 'KEEP' + %char(i);",
    "DESC = *blanks;",
    "OPT = ' ';",
    "rrn = i;",
    "write SFL2;",
    "endfor;",
    "*in31 = *on;",
    "*in34 = *on;",
    "MSG = 'Mark one';",
    "dou *in03;",
    "write FTR2;",
    "exfmt CTL2;",
    "if *in03;",
    "leave;",
    "endif;",
    "n = 0;",
    "readc SFL2;",
    "dow not %eof;",
    "n += 1;",
    "DESC = 'Seen ' + %char(n) + ' opt ' + OPT;",
    "*in50 = *on;",
    "update SFL2;",
    "*in50 = *off;",
    "readc SFL2;",
    "enddo;",
    "MSG = 'Read ' + %char(n);",
    "enddo;",
    "*inlr = *on;",
]

M["DSP013"] = head(["B8L"], ["pgdn", "pgdn", "f3"]) + [
    "dcl-f B8L workstn sfile(SFL3: rrn);",
    "dcl-s rrn packed(4: 0);",
    "dcl-s nxt int(10) inz(1);",
    "dcl-s i int(10);",
    "// A page at a time: ROLLUP comes to the program, which loads the next.",
    "exsr load;",
    "dou *in03;",
    "*in31 = rrn > 0;",
    "exfmt CTL3;",
    "if *in25;",
    "exsr load;",
    "endif;",
    "enddo;",
    "*inlr = *on;",
    "begsr load;",
    "*in33 = *on;",
    "write CTL3;",
    "*in33 = *off;",
    "rrn = 0;",
    "for i = 1 to 4;",
    "LNUM = nxt;",
    "LTXT = 'Line ' + %char(nxt);",
    "nxt += 1;",
    "rrn += 1;",
    "write SFL3;",
    "endfor;",
    "PAGES += 1;",
    "*in34 = PAGES >= 3;",
    "endsr;",
]

M["DSP014"] = head(["B8K"], ["enter", "f3"]) + [
    "dcl-f B8K workstn sfile(SFL2: rrn);",
    "dcl-s rrn packed(4: 0);",
    "dcl-s i int(10);",
    "// CHAIN a subfile record by number, change it, UPDATE; %FOUND.",
    "*in33 = *on;",
    "write CTL2;",
    "*in33 = *off;",
    "for i = 1 to 6;",
    "ITEM = 'ROW' + %char(i);",
    "DESC = 'Before';",
    "rrn = i;",
    "write SFL2;",
    "endfor;",
    "chain 3 SFL2;",
    "if %found(B8K);",
    "DESC = 'After chain ' + ITEM;",
    "update SFL2;",
    "endif;",
    "chain 9 SFL2;",
    "MSG = 'found 9 ' + %found(B8K) + ' item ' + ITEM;",
    "rrn = 5;",
    "chain rrn SFL2;",
    "DESC = 'Five';",
    "update SFL2;",
    "*in31 = *on;",
    "*in34 = *on;",
    "dou *in03;",
    "write FTR2;",
    "exfmt CTL2;",
    "enddo;",
    "*inlr = *on;",
]

M["DSP015"] = head(["B8N"], ["pgdn", "f3"]) + [
    "dcl-f B8N workstn sfile(SFL4: rrn);",
    "dcl-s rrn packed(4: 0);",
    "dcl-s i int(10);",
    "// SFLRCDNBR: the page with record 7 shows first.",
    "*in33 = *on;",
    "write CTL4;",
    "*in33 = *off;",
    "for i = 1 to 12;",
    "NNUM = i;",
    "rrn = i;",
    "write SFL4;",
    "endfor;",
    "TOREC = 7;",
    "dou *in03;",
    "exfmt CTL4;",
    "enddo;",
    "*inlr = *on;",
]

M["DSP016"] = head(["B8D"], ["f3"]) + [
    "dcl-f B8D workstn usropn;",
    "dcl-s s1 char(5);",
    "dcl-s s2 char(5);",
    "dcl-s o1 ind;",
    "dcl-s o2 ind;",
    "// USROPN: OPEN, OPEN again (01215), %OPEN, CLOSE.",
    "o1 = %open(B8D);",
    "open B8D;",
    "open(e) B8D;",
    "s1 = %editc(%status(B8D): 'X');",
    "o2 = %open(B8D);",
    "RES = 'open before ' + o1 + ' after ' + o2 + ' status ' + s1 + ' err ' + %error;",
    "exfmt KEYS;",
    "close B8D;",
    "s2 = %editc(%status(B8D): 'X');",
    "open B8D;",
    "*inlr = *on;",
]

M["DSP017"] = None  # fixed form, below

M["DSP018"] = head(["B8Q"], ["8,20:WXYZ enter", "f12", "f3"]) + [
    "dcl-f B8Q workstn;",
    "// A window record over a background record.",
    "BTXT = 'Behind the window';",
    "dou *in03;",
    "write BACK;",
    "exfmt WIN1;",
    "if *in12;",
    "BTXT = 'Window cancelled';",
    "else;",
    "BTXT = 'Code ' + WCODE;",
    "endif;",
    "write BACK;",
    "exfmt WIN1;",
    "leave;",
    "enddo;",
    "*inlr = *on;",
]

M["DSP019"] = head(["B8R"], ["10,10=1250 enter", "10,10=3 enter", "f3"]) + [
    "dcl-f B8R workstn;",
    "// Numbers with edit codes and an edit word; a typed number.",
    "N1 = -1234.5;",
    "N2 = 98765.43;",
    "N3 = 0;",
    "N4 = 1234567;",
    "N5 = 102326;",
    "N6 = -1.25;",
    "dou *in03;",
    "exfmt NUMS;",
    "N3 = INN * 2;",
    "N1 = INN - 100;",
    "enddo;",
    "*inlr = *on;",
]

M["DSP020"] = head(["CUSTF", "B8S"], ["pgdn", "f3"]) + [
    "dcl-f CUSTF keyed;",
    "dcl-f B8S workstn sfile(CSFL: rrn);",
    "dcl-s rrn packed(4: 0);",
    "// Customers read from the database into a subfile.",
    "*in33 = *on;",
    "write CCTL;",
    "*in33 = *off;",
    "read CUSTF;",
    "dow not %eof(CUSTF);",
    "rrn += 1;",
    "write CSFL;",
    "read CUSTF;",
    "enddo;",
    "TOTAL = rrn;",
    "*in31 = rrn > 0;",
    "*in34 = *on;",
    "dou *in03;",
    "exfmt CCTL;",
    "enddo;",
    "*inlr = *on;",
]

M["DSP021"] = head(["B8T"], ["3,10=FIRST enter", "3,10=SECOND enter", "f3"]) + [
    "dcl-f B8T workstn;",
    "// WRITE then READ by format, then READ by file name.",
    "SEEN = 'Nothing seen';",
    "write ASK;",
    "read ASK;",
    "SEEN = 'By format: ' + WORD + ' in03 ' + *in03;",
    "write ASK;",
    "read B8T;",
    "SEEN = 'By file: ' + WORD + ' in03 ' + *in03;",
    "dou *in03;",
    "exfmt ASK;",
    "enddo;",
    "*inlr = *on;",
]


M["DSP022"] = ["**FREE", "//; dds: B8P", "ctl-opt dftactgrp(*no);"] + [
    "dcl-f B8P printer oflind(*in90);",
    "dcl-s i int(10);",
    "// An externally described printer file: formats, an option indicator.",
    "write HDR;",
    "for i = 1 to 8;",
    "DNUM = i;",
    "DNAME = 'Name ' + %char(i);",
    "DAMT = i * 12.5;",
    "*in50 = %rem(i: 3) = 0;",
    "TAMT += DAMT;",
    "write DTL;",
    "if *in90;",
    "write HDR;",
    "*in90 = *off;",
    "endif;",
    "endfor;",
    "write TOT;",
    "*inlr = *on;",
]


def fixed_dsp017():
    """DSP017: fixed form, the file and its indicators in columns."""
    def c(cond="", f1="", op="", f2="", res="", ind=""):
        s = list(" " * 80)
        def put(at, t, w):
            s[at - 1:at - 1 + w] = str(t).ljust(w)[:w]
        put(6, "C", 1)
        put(9, cond, 3)
        put(12, f1, 14)
        put(26, op, 10)
        # Extended factor 2 runs to column 80.
        put(36, f2, 14 if res or ind else 45)
        if res:
            put(50, res, 14)
        if ind:
            put(71, ind, 6)
        return "".join(s).rstrip()
    return [
        "      *; dds: B8D",
        "      *; keys: 3,10=FIX f5",
        "      *; keys: f12",
        "      *; keys: f3",
        "     H DFTACTGRP(*NO)",
        "     FB8D       CF   E             WORKSTN",
        "      * Fixed form: EXFMT with the key indicators.",
        c(op="EVAL", f2="RES = 'Fixed form'"),
        c(f1="*IN03", op="DOUEQ", f2="*ON"),
        c(op="EXFMT", f2="KEYS"),
        c(cond=" KE", op="EVAL", f2="RES = 'KE ' + VAL"),
        c(cond=" KL", op="EVAL", f2="RES = 'KL ' + VAL"),
        c(op="ENDDO"),
        c(op="SETON", ind="LR"),
    ]


def main():
    os.makedirs(OUT, exist_ok=True)
    M["DSP017"] = fixed_dsp017()
    for name, lines in M.items():
        for i, l in enumerate(lines):
            assert len(l) <= 100, (name, i, l)
        with open(os.path.join(OUT, name + ".rpgle"), "w", newline="\n") as f:
            f.write("\n".join(lines) + "\n")
    print(f"{len(M)} members")


if __name__ == "__main__":
    main()
