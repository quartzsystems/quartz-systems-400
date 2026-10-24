# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.

"""The feature matrix of docs/PHASE9.md: every operation code, built-in
function and keyword of ILE RPG 7.5, whether the compiler knows it, and
the corpus members with an oracle outcome that use it (and so verify it
on pub400, V7R5M0). Writes the matrix between the markers in PHASE9.md.

    python -I tools/conformance/rpg/gen/matrix.py
"""

import os
import re

ROOT = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", "..", ".."))
RPG = os.path.join(ROOT, "tools", "conformance", "rpg")
DOC = os.path.join(ROOT, "docs", "PHASE9.md")

OPCODES = """ACQ ADD ADDDUR ALLOC ANDxx BEGSR BITOFF BITON CABxx CALL CALLB CALLP CASxx CAT
CHAIN CHECK CHECKR CLEAR CLOSE COMMIT COMP DATA-GEN DATA-INTO DEALLOC DEFINE DELETE DIV DO
DOU DOUxx DOW DOWxx DSPLY DUMP ELSE ELSEIF ENDCS ENDDO ENDFOR ENDIF ENDMON ENDSL ENDSR EVAL
EVALR EVAL-CORR EXCEPT EXFMT EXSR EXTRCT FEOD FOR FOR-EACH FORCE GOTO IF IFxx IN ITER KFLD
KLIST LEAVE LEAVESR LOOKUP MHHZO MHLZO MLHZO MLLZO MONITOR MOVE MOVEA MOVEL MULT MVR NEXT
OCCUR ON-ERROR ON-EXCP ON-EXIT OPEN ORxx OTHER OUT PARM PLIST POST READ READC READE READP
READPE REALLOC REL RESET RETURN ROLBK SCAN SELECT SETGT SETLL SETOFF SETON SHTDN SORTA SQRT
SUB SUBDUR SUBST TAG TEST TESTB TESTN TESTZ TIME UNLOCK UPDATE WHEN WHENxx WHEN-IN WHEN-IS
WRITE XFOOT XLATE XML-INTO XML-SAX Z-ADD Z-SUB""".split()

BIFS = """ABS ADDR ALLOC BITAND BITNOT BITOR BITXOR CHAR CHARCOUNT CHECK CHECKR CONCAT
CONCATARR DATA DATE DAYS DEC DECH DECPOS DIFF DIV EDITC EDITFLT EDITW ELEM EOF EQUAL ERROR
FIELDS FLOAT FOUND GEN GRAPH HANDLER HIVAL HOURS INT INTH KDS LEFT LEN LIST LOOKUP LOOKUPLT
LOOKUPLE LOOKUPGT LOOKUPGE LOVAL LOWER MAX MAXARR MIN MINARR MINUTES MONTHS MSECONDS
NULLIND OCCUR OMITTED OPEN PADDR PARMNUM PARMS PARSER PASSED PROC RANGE REALLOC REM REPLACE
RIGHT SCAN SCANR SCANRPL SECONDS SHTDN SIZE SPLIT SQRT STATUS STR SUBARR SUBDT SUBST TARGET
THIS TIME TIMESTAMP TLOOKUP TLOOKUPLT TLOOKUPLE TLOOKUPGT TLOOKUPGE TRIM TRIML TRIMR UCS2
UNS UNSH UPPER XFOOT XLATE XML YEARS""".split()

KEYWORDS = {
    "Control": """ACTGRP ALLOC ALTSEQ ALWNULL AUT BNDDIR CCSID CCSIDCVT COPYNEST COPYRIGHT
CURSYM CVTOPT DATEDIT DATFMT DCLOPT DEBUG DECEDIT DECPREC DFTACTGRP DFTNAME ENBPFRCOL
EXPROPTS EXTBININT FIXNBR FLTDIV FORMSALIGN FTRANS GENLVL INDENT INTPREC LANGID MAIN NOMAIN
OPENOPT OPTIMIZE OPTION PGMINFO PRFDTA REQPREXP SRTSEQ STGMDL TEXT THREAD TIMFMT TRUNCNBR
USRPRF VALIDATE""".split(),
    "File": """ALIAS BLOCK COMMIT DATFMT DEVID EXTDESC EXTFILE EXTIND EXTMBR FORMLEN FORMOFL
HANDLER IGNORE INCLUDE INDDS INFDS INFSR KEYED KEYLOC LIKEFILE MAXDEV OFLIND PASS PGMNAME
PLIST PREFIX PRTCTL QUALIFIED RAFDATA RECNO RENAME SAVEDS SAVEIND SFILE SLN STATIC TEMPLATE
TIMFMT USAGE USROPN""".split(),
    "Definition": """ALIAS ALIGN ALT ALTSEQ ASCEND BASED CCSID CLASS CONST CTDATA DATFMT DESCEND
DIM DTAARA EXPORT EXT EXTFLD EXTFMT EXTNAME EXTPGM EXTPROC FROMFILE IMPORT INZ LEN LIKE
LIKEDS LIKEFILE LIKEREC NOOPT NULLIND OCCURS OPDESC OPTIONS OVERLAY PACKEVEN PERRCD POS
PREFIX PSDS QUALIFIED RTNPARM SAMEPOS STATIC TEMPLATE TIMFMT TOFILE VALUE VARYING""".split(),
    "Procedure": "EXPORT PGMINFO REQPROTO RTNPARM SERIALIZE STATIC".split(),
}

# Free-form spellings of operation families.
FREE = {"IFxx": "IF", "DOUxx": "DOU", "DOWxx": "DOW", "WHENxx": "WHEN"}


def members():
    """(name, text, outcome) of every member with an oracle outcome."""
    out = []
    for d in sorted(os.listdir(RPG)):
        p = os.path.join(RPG, d)
        if not os.path.isdir(p) or d in ("expected", "oracle", "perf", "gen", "dds"):
            continue
        for f in sorted(os.listdir(p)):
            stem, ext = os.path.splitext(f)
            if ext.lower() not in (".rpgle", ".sqlrpgle"):
                continue
            exp = os.path.join(RPG, "expected", stem + ".out")
            if not os.path.exists(exp):
                continue
            text = open(os.path.join(p, f), encoding="utf-8").read()
            out.append((stem, text, open(exp, encoding="utf-8").read()))
    return out


def code_words(text):
    """The member without comments, upper case, and its operation codes."""
    lines, ops = [], set()
    free = text.lstrip().upper().startswith("**FREE")
    in_free = False
    for raw in text.splitlines():
        u = raw.upper()
        if u.startswith("**") and not u.startswith("**FREE"):
            break  # compile-time data
        if not free and len(u) > 6 and u[6] == "*":
            continue
        if not free and u[5:6] == " " and u[6:].strip() in ("/FREE", "/END-FREE"):
            in_free = u[6:].strip() == "/FREE"
            continue
        body = u.split("//")[0]
        if not free and not in_free and u[5:6] == "C" and len(u) > 25:
            op = u[25:35].split("(")[0].strip()
            if op:
                ops.add(op)
        elif free or in_free or (not free and u[5:6] == " " and u[6:].strip()):
            for stmt in body.split(";"):
                w = re.match(r"\s*([A-Z][A-Z0-9-]*)", stmt)
                if w:
                    ops.add(w.group(1))
        lines.append(body)
    return "\n".join(lines), ops


def compiler_text():
    parts = []
    for top in ("rpg",):
        for dp, _, fs in os.walk(os.path.join(ROOT, top)):
            for f in fs:
                if f.endswith(".rs"):
                    parts.append(open(os.path.join(dp, f), encoding="utf-8").read().upper())
    return "\n".join(parts)


def main():
    ms = [(n, *code_words(t), o) for n, t, o in members()]
    src = compiler_text()

    def known(word):
        return f'"{word}"' in src or f'"%{word}"' in src or f'"{word}(' in src

    def row(label, users, impl):
        ok = [n for n in users]
        if ok:
            shown = ", ".join(ok[:4]) + (f" and {len(ok) - 4} more" if len(ok) > 4 else "")
            return f"| {label} | verified, V7R5M0 | {shown} |"
        return f"| {label} | {'compiled, not in the corpus' if impl else 'not supported'} | |"

    out = ["<!-- matrix: generated by tools/conformance/rpg/gen/matrix.py -->", ""]
    out += ["### Operation codes", "", "| Operation | Status | Members |", "|---|---|---|"]
    for op in OPCODES:
        base = op.replace("xx", "")
        names = {base, FREE.get(op, base)}
        users = [n for n, _, ops, o in ms if any(x in ops or (x != base and any(y.startswith(base) and len(y) == len(base) + 2 for y in ops)) for x in names) and "created: yes" in o]
        if op.endswith("xx"):
            users = [n for n, _, ops, o in ms if any(y.startswith(base) and len(y) == len(base) + 2 for y in ops) and "created: yes" in o]
        out.append(row(f"`{op}`", users, known(base) or known(base + "EQ")))
    out += ["", "### Built-in functions", "", "| Function | Status | Members |", "|---|---|---|"]
    for b in BIFS:
        pat = re.compile(r"%" + re.escape(b) + r"\b(?!-)")
        users = [n for n, t, _, o in ms if pat.search(t) and "created: yes" in o]
        out.append(row(f"`%{b}`", users, known(b) or known("%" + b)))
    for group, kws in KEYWORDS.items():
        out += ["", f"### {group} keywords", "", "| Keyword | Status | Members |", "|---|---|---|"]
        for k in kws:
            pat = re.compile(r"(?<![A-Z0-9%*-])" + re.escape(k) + r"(?![A-Z0-9-])\s*(\(|;|\s|$)", re.M)
            users = [n for n, t, _, o in ms if pat.search(t) and "created: yes" in o]
            out.append(row(f"`{k}`", users, known(k)))
    out += ["", "<!-- end matrix -->"]
    doc = open(DOC, encoding="utf-8").read()
    start = doc.find("<!-- matrix")
    end = doc.find("<!-- end matrix -->")
    block = "\n".join(out)
    if start >= 0 and end >= 0:
        doc = doc[:start] + block + doc[end + len("<!-- end matrix -->"):]
    else:
        doc = doc.rstrip("\n") + "\n\n## Feature matrix\n\n" + block + "\n"
    open(DOC, "w", encoding="utf-8", newline="\n").write(doc)
    print(f"{len(ms)} members read")


main()
