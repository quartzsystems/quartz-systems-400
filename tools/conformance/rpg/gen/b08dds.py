"""Corpus batch 8's display files: writes ../dds/B8*.dspf.

Each line is built by columns (DDS form A): conditioning 7-16, the
record letter 17, the name 19-28, reference 29, length 30-34, type 35,
decimals 36-37, usage 38, line 39-41, position 42-44, keywords 45-80.

usage: python b08dds.py
"""
import os

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "dds")


def a(name="", ln="", typ="", dec="", use="", row="", col="", kw="", cond="", rec=False, ref=""):
    """One DDS line."""
    s = list(" " * 80)

    def put(at, text, width, right=False):
        text = str(text)
        assert len(text) <= width, (text, width)
        text = text.rjust(width) if right else text.ljust(width)
        s[at - 1:at - 1 + width] = text

    put(6, "A", 1)
    # Conditioning: up to three indicators, each N (col 8) and the
    # number (cols 9-10) in its own three columns.
    for k, w in enumerate(cond.split()):
        at = 8 + 3 * k
        if w.startswith("N"):
            put(at, "N", 1)
            w = w[1:]
        put(at + 1, w, 2)
    if rec:
        put(17, "R", 1)
    put(19, name, 10)
    put(29, ref, 1)
    put(30, ln, 5, right=True)
    put(35, typ, 1)
    put(36, dec, 2, right=True)
    put(38, use, 1)
    put(39, row, 3, right=True)
    put(42, col, 3, right=True)
    line = "".join(s[:44]) + kw
    assert len(line) <= 80, line
    return line.rstrip()


def kw(text, cond=""):
    """A keyword line."""
    return a(kw=text, cond=cond)


def rec(name, keywords=""):
    """A record format line."""
    return a(name=name, rec=True, kw=keywords)


def const(row, col, text, keywords="", cond=""):
    """A constant."""
    return a(row=row, col=col, kw=f"'{text}'" + (keywords and " " + keywords), cond=cond)


def fld(name, ln, typ, use, row="", col="", dec="", keywords="", cond="", ref=""):
    """A named field."""
    return a(name=name, ln=ln, typ=typ, dec=dec, use=use, row=row, col=col, kw=keywords, cond=cond, ref=ref)


FILES = {}

# B8A: one record, input, output and both fields, CA03.
FILES["B8A"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    rec("SCR1"),
    const(1, 2, "B8A first screen"),
    const(3, 2, "Name:"),
    fld("NAME", 10, "A", "B", 3, 10),
    const(4, 2, "Count:"),
    fld("CNT", 5, "Y", "B", 4, 10, dec=0, keywords="EDTCDE(Z)"),
    fld("MSG", 40, "A", "O", 6, 2),
]

# B8B: several formats; OVERLAY keeps the header and footer.
FILES["B8B"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    rec("HDR"),
    const(1, 2, "B8B several formats"),
    fld("HTXT", 30, "A", "O", 2, 2),
    rec("QRY", "OVERLAY"),
    kw("CF05(05 'Refresh')"),
    const(5, 2, "Code:"),
    fld("CODE", 4, "A", "I", 5, 8),
    const(6, 2, "Qty:"),
    fld("QTY", 3, "Y", "I", 6, 8, dec=0),
    rec("FTR", "OVERLAY"),
    fld("RESULT", 40, "A", "O", 21, 2),
    const(23, 2, "F3=Exit   F5=Refresh"),
]

# B8C: option indicators on constants, fields and attributes.
FILES["B8C"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    rec("SCR"),
    const(1, 2, "B8C indicators"),
    const(3, 2, "Indicator 40 is off", cond="N40"),
    const(3, 2, "Indicator 40 is on", cond="40"),
    const(5, 2, "Option:"),
    fld("OPT", 1, "A", "B", 5, 10, cond="41"),
    fld("STAT", 20, "A", "O", 7, 2),
    kw("DSPATR(HI)", cond="42"),
    const(8, 2, "Shown with 43", cond="43"),
    const(9, 2, "Shown with 43 and 44", cond="43 44"),
    fld("COUNT", 3, "S", "O", 10, 2, dec=0),
]

# B8D: command function keys (data back) and command attention keys.
FILES["B8D"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    kw("CF05(05 'Data')"),
    kw("CA12(12 'Cancel')"),
    rec("KEYS"),
    kw("CF10(10)"),
    const(1, 2, "B8D keys"),
    const(3, 2, "Value:"),
    fld("VAL", 10, "A", "B", 3, 10),
    fld("RES", 70, "A", "O", 5, 2),
    fld("RES2", 70, "A", "O", 6, 2),
]

# B8E: ERRMSG on the fields, set off when read.
FILES["B8E"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    rec("ENTRY"),
    const(1, 2, "B8E error messages"),
    const(3, 2, "Amount:"),
    fld("AMT", 5, "Y", "B", 3, 10, dec=0),
    kw("ERRMSG('Amount over 99' 50)", cond="50"),
    const(4, 2, "Note:"),
    fld("NOTE", 20, "A", "B", 4, 10),
    kw("ERRMSG('Note is required' 51)", cond="51"),
    fld("DONE", 40, "A", "O", 6, 2),
]

# B8F: ERRMSGID from a message file.
FILES["B8F"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    rec("ENTRY"),
    const(1, 2, "B8F message identifiers"),
    const(3, 2, "Code:"),
    fld("CODE", 4, "A", "B", 3, 10),
    kw("ERRMSGID(USR8001 B8MSGF 52)", cond="52"),
    fld("DONE", 40, "A", "O", 6, 2),
]

# B8H: a separate indicator area.
FILES["B8H"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("INDARA"),
    kw("CA03(03 'Exit')"),
    rec("SCR"),
    kw("CF06(06)"),
    const(1, 2, "B8H indicator area"),
    const(3, 2, "Thirty on", cond="30"),
    const(3, 2, "Thirty off", cond="N30"),
    const(4, 2, "Input:"),
    fld("INP", 5, "A", "B", 4, 10),
    fld("RES", 60, "A", "O", 6, 2),
    const(7, 2, "Shown with 31", cond="31"),
]

# B8J: a subfile, loaded whole, the system rolling its pages.
FILES["B8J"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    rec("SFL1", "SFL"),
    fld("SNUM", 3, "Y", "O", 6, 2, dec=0, keywords="EDTCDE(Z)"),
    fld("SNAM", 20, "A", "O", 6, 8),
    rec("CTL1", "SFLCTL(SFL1)"),
    kw("SFLSIZ(0020)"),
    kw("SFLPAG(0004)"),
    kw("OVERLAY"),
    kw("SFLDSP", cond="31"),
    kw("SFLDSPCTL", cond="32"),
    kw("SFLCLR", cond="33"),
    kw("SFLEND(*MORE)", cond="34"),
    const(1, 2, "B8J load all"),
    const(5, 2, "Num"),
    const(5, 8, "Name"),
    rec("FTR", "OVERLAY"),
    const(23, 2, "F3=Exit"),
]

# B8K: subfile options read back with READC; SFLNXTCHG.
FILES["B8K"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    rec("SFL2", "SFL"),
    kw("SFLNXTCHG", cond="50"),
    fld("OPT", 1, "A", "B", 6, 2),
    fld("ITEM", 10, "A", "O", 6, 6),
    fld("DESC", 20, "A", "O", 6, 17),
    rec("CTL2", "SFLCTL(SFL2)"),
    kw("SFLSIZ(0010)"),
    kw("SFLPAG(0005)"),
    kw("OVERLAY"),
    kw("SFLDSP", cond="31"),
    kw("SFLDSPCTL"),
    kw("SFLCLR", cond="33"),
    kw("SFLEND(*MORE)", cond="34"),
    const(1, 2, "B8K options"),
    const(3, 2, "Type 1 to mark, 4 to drop."),
    const(5, 2, "Opt"),
    const(5, 6, "Item"),
    rec("FTR2", "OVERLAY"),
    fld("MSG", 40, "A", "O", 21, 2),
]

# B8L: a page at a time, the program loading each page.
FILES["B8L"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    rec("SFL3", "SFL"),
    fld("LNUM", 4, "Y", "O", 6, 2, dec=0),
    fld("LTXT", 20, "A", "O", 6, 8),
    rec("CTL3", "SFLCTL(SFL3)"),
    kw("SFLSIZ(0004)"),
    kw("SFLPAG(0004)"),
    kw("ROLLUP(25)"),
    kw("OVERLAY"),
    kw("SFLDSP", cond="31"),
    kw("SFLDSPCTL"),
    kw("SFLCLR", cond="33"),
    kw("SFLEND(*MORE)", cond="34"),
    const(1, 2, "B8L page at a time"),
    fld("PAGES", 3, "Y", "O", 2, 2, dec=0),
]

# B8N: SFLRCDNBR puts a record's page on the display.
FILES["B8N"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    rec("SFL4", "SFL"),
    fld("NNUM", 3, "Y", "O", 6, 2, dec=0),
    rec("CTL4", "SFLCTL(SFL4)"),
    kw("SFLSIZ(0012)"),
    kw("SFLPAG(0004)"),
    kw("OVERLAY"),
    kw("SFLDSP"),
    kw("SFLDSPCTL"),
    kw("SFLCLR", cond="33"),
    const(1, 2, "B8N record number"),
    fld("TOREC", 4, "S", "H", dec=0, keywords="SFLRCDNBR"),
]

# B8Q: a window over a background record.
FILES["B8Q"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    rec("BACK"),
    const(1, 2, "B8Q background"),
    fld("BTXT", 30, "A", "O", 3, 2),
    rec("WIN1", "WINDOW(5 10 6 40)"),
    kw("CA12(12)"),
    const(1, 2, "In a window"),
    const(3, 2, "Code:"),
    fld("WCODE", 4, "A", "B", 3, 9),
]

# B8R: numbers shown with edit codes and words, and typed.
FILES["B8R"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    rec("NUMS"),
    const(1, 2, "B8R numbers"),
    fld("N1", 9, "Y", "O", 3, 2, dec=2, keywords="EDTCDE(J)"),
    fld("N2", 9, "Y", "O", 4, 2, dec=2, keywords="EDTCDE(1)"),
    fld("N3", 9, "Y", "O", 5, 2, dec=2, keywords="EDTCDE(Z)"),
    fld("N4", 7, "Y", "O", 6, 2, dec=0, keywords="EDTCDE(3)"),
    fld("N5", 6, "Y", "O", 7, 2, dec=0, keywords="EDTWRD('  /  /  ')"),
    fld("N6", 5, "S", "O", 8, 2, dec=2),
    const(10, 2, "Type:"),
    fld("INN", 7, "Y", "B", 10, 10, dec=2),
]

# B8T: WRITE then READ, by format and by file name.
FILES["B8T"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    rec("ASK"),
    const(1, 2, "B8T write then read"),
    const(3, 2, "Word:"),
    fld("WORD", 8, "A", "I", 3, 10),
    fld("SEEN", 50, "A", "O", 5, 2),
]

# B8S: customers from the database in a subfile.
FILES["B8S"] = [
    kw("DSPSIZ(24 80 *DS3)"),
    kw("CA03(03 'Exit')"),
    kw("REF(CUSTF)"),
    rec("CSFL", "SFL"),
    fld("CUSNO", "", "", "O", 5, 2, ref="R"),
    fld("NAME", "", "", "O", 5, 10, ref="R"),
    fld("CITY", "", "", "O", 5, 32, ref="R"),
    rec("CCTL", "SFLCTL(CSFL)"),
    kw("SFLSIZ(0050)"),
    kw("SFLPAG(0006)"),
    kw("OVERLAY"),
    kw("SFLDSP", cond="31"),
    kw("SFLDSPCTL"),
    kw("SFLCLR", cond="33"),
    kw("SFLEND(*MORE)", cond="34"),
    const(1, 2, "B8S customers"),
    fld("TOTAL", 5, "Y", "O", 2, 2, dec=0, keywords="EDTCDE(Z)"),
]

# B8P: an externally described printer file (written as B8P.prtf).
PRINTERS = {}
PRINTERS["B8P"] = [
    rec("HDR", "SKIPB(1)"),
    a(col=2, kw="'B8P report'"),
    a(col=40, kw="'Page'"),
    a(col=45, kw="PAGNBR"),
    kw("SPACEA(2)"),
    rec("DTL", "SPACEA(1)"),
    fld("DNUM", 5, "S", "", "", 2, dec=0),
    fld("DNAME", 20, "A", "", "", 10),
    a(col=32, kw="'*'", cond="50"),
    fld("DAMT", 9, "S", "", "", 35, dec=2, keywords="EDTCDE(J)"),
    rec("TOT", "SPACEB(1)"),
    a(col=2, kw="'Total'"),
    fld("TAMT", 11, "S", "", "", 33, dec=2, keywords="EDTCDE(J)"),
]


def main():
    for name, lines in FILES.items():
        with open(os.path.join(OUT, name + ".dspf"), "w", newline="\n") as f:
            f.write("\n".join(lines) + "\n")
    for name, lines in PRINTERS.items():
        with open(os.path.join(OUT, name + ".prtf"), "w", newline="\n") as f:
            f.write("\n".join(lines) + "\n")
    print(f"{len(FILES)} display files, {len(PRINTERS)} printer files")


if __name__ == "__main__":
    main()
