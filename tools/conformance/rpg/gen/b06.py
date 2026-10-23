"""Corpus batch 6: the RPG cycle with primary and secondary files,
record identifying indicators, control levels, matching records and
overflow. Writes ../b06cycle/*.rpgle; the files come from ../dds.

usage: python b06.py
"""
import os

from fixed import C, CX, D, F, I_fld, I_rec, I_xfld, I_xrec, O_fld, O_rec, comment, place

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b06cycle")
SQLS = {"CUSTF", "ORDF", "HISTF", "TYPESF", "SALESF", "TRANF", "MSTF", "ACTF"}


def files(*names):
    out = []
    for f in names:
        out.append(place([(7, "*"), (8, f"; dds: {f}")]))
        if f in SQLS:
            out.append(place([(7, "*"), (8, f"; sqlfile: {f}")]))
    return out


def O_or(cond):
    """An OR line of output conditioning."""
    return place([(6, "O"), (16, "OR"), (21, cond)])


PRT = F("QSYSPRT", "O", fmt="F", rlen="132", dev="PRINTER", kw="OFLIND(*INOF)")

# SALESF as a program-described file (22 bytes).
SALES_I = [
    I_fld("REGION", 1, 2),
    I_fld("REP", 3, 6),
    I_fld("ITEM", 7, 16),
    I_fld("AMT", 17, 20, 2, fmt="P"),
    I_fld("QTY", 21, 22, 0, fmt="P"),
]

M = {}

M["CYC001"] = files("SALESF") + [
    comment("A primary file and detail output only: no calculations."),
    F("SALESF", "I", "P", fmt="F", rlen="22", dev="DISK"),
    PRT,
    I_rec("SALESF", "NS", "01"),
] + SALES_I + [
    O_rec("QSYSPRT", "D", cond=" 01", sa=1),
    O_fld("REGION", 4),
    O_fld("REP", 10),
    O_fld("ITEM", 22),
    O_fld("AMT", 34, edit="2"),
    O_fld("QTY", 40, edit="Z"),
]

M["CYC002"] = files("SALESF") + [
    comment("An externally described primary file; LR totals."),
    F("SALESF", "I", "P", fmt="E", dev="DISK"),
    PRT,
    D("TOTAL", "S", to="9", typ="P", dec="2"),
    D("COUNT", "S", to="5", typ="P", dec="0"),
    I_xrec("SALESR", "01"),
    C(cond=" 01", op="ADD", f2="AMT", res="TOTAL"),
    C(cond=" 01", op="ADD", f2="1", res="COUNT"),
    C(level="LR", op="Z-ADD", f2="TOTAL", res="AVG", ln="9", dec="2"),
    C(level="LR", f1="COUNT", op="IFGT", f2="0"),
    CX("EVAL(H)", "AVG = TOTAL / COUNT", level="LR"),
    C(level="LR", op="ENDIF"),
    O_rec("QSYSPRT", "H", cond=" 1P", sa=2),
    O_fld(const="'Region Rep  Item'", end=17),
    O_fld(const="'Amount'", end=34),
    O_rec("QSYSPRT", "D", cond=" 01", sa=1),
    O_fld("REGION", 2),
    O_fld("REP", 9),
    O_fld("ITEM", 21),
    O_fld("AMT", 34, edit="J"),
    O_rec("QSYSPRT", "T", cond=" LR", sb=1),
    O_fld("COUNT", 5, edit="Z"),
    O_fld(const="'records'", end=14),
    O_fld("TOTAL", 34, edit="J"),
    O_fld("AVG", 48, edit="J"),
]

M["CYC003"] = files("SALESF") + [
    comment("One control level: region totals at L1, a grand total at LR."),
    F("SALESF", "I", "P", fmt="F", rlen="22", dev="DISK"),
    PRT,
    D("REGTOT", "S", to="9", typ="P", dec="2"),
    D("GRAND", "S", to="11", typ="P", dec="2"),
    I_rec("SALESF", "NS", "01"),
    I_fld("REGION", 1, 2, level="L1"),
    I_fld("REP", 3, 6),
    I_fld("ITEM", 7, 16),
    I_fld("AMT", 17, 20, 2, fmt="P"),
    I_fld("QTY", 21, 22, 0, fmt="P"),
    C(cond=" 01", op="ADD", f2="AMT", res="REGTOT"),
    C(level="L1", op="ADD", f2="REGTOT", res="GRAND"),
    O_rec("QSYSPRT", "D", cond=" 01", sa=1),
    O_fld("REGION", 4),
    O_fld("ITEM", 16),
    O_fld("AMT", 28, edit="J"),
    O_rec("QSYSPRT", "D", cond=" L1", sb=1, sa=1),
    O_fld(const="'New region'", end=12),
    O_fld("REGION", 15),
    O_rec("QSYSPRT", "T", cond=" L1", sa=1),
    O_fld(const="'Total for'", end=10),
    O_fld("REGION", 13),
    O_fld("REGTOT", 28, edit="J", blank="B"),
    O_rec("QSYSPRT", "T", cond=" LR", sb=1),
    O_fld(const="'Grand total'", end=11),
    O_fld("GRAND", 28, edit="J"),
]

M["CYC004"] = files("SALESF") + [
    comment("Two control levels: L2 region, L1 rep."),
    F("SALESF", "I", "P", fmt="F", rlen="22", dev="DISK"),
    PRT,
    D("REPTOT", "S", to="9", typ="P", dec="2"),
    D("REGTOT", "S", to="9", typ="P", dec="2"),
    D("NREC", "S", to="3", typ="P", dec="0"),
    I_rec("SALESF", "NS", "01"),
    I_fld("REGION", 1, 2, level="L2"),
    I_fld("REP", 3, 6, level="L1"),
    I_fld("ITEM", 7, 16),
    I_fld("AMT", 17, 20, 2, fmt="P"),
    I_fld("QTY", 21, 22, 0, fmt="P"),
    C(cond=" L1", op="Z-ADD", f2="0", res="NREC"),
    C(cond=" 01", op="ADD", f2="AMT", res="REPTOT"),
    C(cond=" 01", op="ADD", f2="1", res="NREC"),
    C(level="L1", op="ADD", f2="REPTOT", res="REGTOT"),
    O_rec("QSYSPRT", "T", cond=" L1", sa=1),
    O_fld("REGION", 4),
    O_fld("REP", 10),
    O_fld("NREC", 14, edit="Z"),
    O_fld("REPTOT", 28, edit="J", blank="B"),
    O_rec("QSYSPRT", "T", cond=" L2", sa=2),
    O_fld("REGION", 4),
    O_fld(const="'**'", end=10),
    O_fld("REGTOT", 28, edit="J", blank="B"),
    O_fld(const="'L1 L2 LR'", end=40),
    O_rec("QSYSPRT", "T", cond=" LR"),
    O_fld(const="'end'", end=3),
]

M["CYC005"] = files("SALESF") + [
    comment("Overflow: headings conditioned by 1P or OF, page numbers."),
    F("SALESF", "I", "P", fmt="F", rlen="22", dev="DISK"),
    PRT,
    I_rec("SALESF", "NS", "01"),
] + SALES_I + [
    O_rec("QSYSPRT", "H", cond=" 1P", kb=1, sa=2),
    O_or(" OF"),
    O_fld(const="'Sales by line'", end=13),
    O_fld(const="'Page'", end=40),
    O_fld("PAGE", 45, edit="Z"),
    O_rec("QSYSPRT", "D", cond=" 01", sa=3),
    O_fld("REGION", 4),
    O_fld("ITEM", 16),
    O_fld("AMT", 28, edit="J"),
    O_rec("QSYSPRT", "D", cond=" 01", sa=3),
    O_fld(const="'again'", end=9),
    O_fld("REP", 16),
    O_rec("QSYSPRT", "T", cond=" LR", sb=1),
    O_fld(const="'Last page'", end=9),
    O_fld("PAGE", 15, edit="Z"),
]

M["CYC006"] = files("CUSTF") + [
    comment("1P output, detail calculations and a halt on LR set early."),
    F("CUSTF", "I", "P", fmt="E", rtype="K", dev="DISK"),
    PRT,
    D("N", "S", to="3", typ="P", dec="0"),
    C(op="ADD", f2="1", res="N"),
    C(f1="N", op="IFEQ", f2="5"),
    C(op="SETON", hi="LR"),
    C(op="ENDIF"),
    O_rec("QSYSPRT", "H", cond=" 1P", sa=1),
    O_fld(const="'First page only'", end=15),
    O_rec("QSYSPRT", "D", cond="N1P", sa=1),
    O_fld("N", 3, edit="Z"),
    O_fld("CUSNO", 10, edit="Z"),
    O_fld("NAME", 32),
    O_fld("BAL", 46, edit="K"),
    O_rec("QSYSPRT", "T", cond=" LR"),
    O_fld(const="'Stopped after'", end=14),
    O_fld("N", 18, edit="Z"),
]

M["CYC007"] = files("TRANF") + [
    comment("Record identifying indicators from record identification codes."),
    F("TRANF", "I", "P", fmt="F", rlen="20", dev="DISK"),
    PRT,
    I_rec("TRANF", "AA", "11", codes=[(1, "", "C", "H")]),
    I_fld("HCUS", 2, 4, 0),
    I_fld("HNAME", 5, 20),
    I_rec("TRANF", "AB", "12", codes=[(1, "", "C", "D")]),
    I_fld("DAMT", 2, 8, 2),
    I_fld("DITEM", 9, 20),
    I_rec("TRANF", "AC", "13"),
    I_fld("RAW", 1, 20),
    C(cond=" 12", op="ADD", f2="DAMT", res="CUSTOT", ln="9", dec="2"),
    O_rec("QSYSPRT", "D", cond=" 11", sb=1, sa=1),
    O_fld("HCUS", 5),
    O_fld("HNAME", 22),
    O_rec("QSYSPRT", "D", cond=" 12", sa=1),
    O_fld("DITEM", 16),
    O_fld("DAMT", 28, edit="J"),
    O_fld("CUSTOT", 42, edit="J"),
    O_rec("QSYSPRT", "D", cond=" 13", sa=1),
    O_fld(const="'other:'", end=6),
    O_fld("RAW", 27),
]

M["CYC008"] = files("MSTF", "ACTF") + [
    comment("Matching records: masters primary, activity secondary, M1, MR."),
    F("MSTF", "I", "P", fmt="F", rlen="13", dev="DISK"),
    F("ACTF", "I", "S", fmt="F", rlen="8", dev="DISK"),
    PRT,
    I_rec("MSTF", "AA", "01"),
    I_fld("MCUS", 1, 3, match="M1"),
    I_fld("MNAME", 4, 13),
    I_rec("ACTF", "BB", "02"),
    I_fld("ACUS", 1, 3, match="M1"),
    I_fld("AAMT", 4, 8, 0),
    O_rec("QSYSPRT", "D", cond=" 01 MR", sa=1),
    O_fld("MCUS", 3),
    O_fld("MNAME", 15),
    O_fld(const="'master with activity'", end=37),
    O_rec("QSYSPRT", "D", cond=" 01NMR", sa=1),
    O_fld("MCUS", 3),
    O_fld("MNAME", 15),
    O_fld(const="'master alone'", end=29),
    O_rec("QSYSPRT", "D", cond=" 02 MR", sa=1),
    O_fld("ACUS", 7),
    O_fld("AAMT", 15, edit="Z"),
    O_fld(const="'matched'", end=24),
    O_rec("QSYSPRT", "D", cond=" 02NMR", sa=1),
    O_fld("ACUS", 7),
    O_fld("AAMT", 15, edit="Z"),
    O_fld(const="'unmatched'", end=26),
]

M["CYC009"] = files("SALESF") + [
    comment("Fetch overflow on detail lines, totals at L1 and LR."),
    F("SALESF", "I", "P", fmt="E", dev="DISK"),
    PRT,
    D("REGTOT", "S", to="9", typ="P", dec="2"),
    I_xrec("SALESR", "01"),
    I_xfld("REGION", level="L1"),
    C(cond=" 01", op="ADD", f2="AMT", res="REGTOT"),
    O_rec("QSYSPRT", "H", cond=" 1P", sa=2),
    O_or(" OF"),
    O_fld(const="'Region report'", end=13),
    O_fld("PAGE", 30, edit="Z"),
    O_rec("QSYSPRT", "D", "F", cond=" 01", sa=4),
    O_fld("REGION", 4),
    O_fld("REP", 10),
    O_fld("ITEM", 22),
    O_fld("AMT", 34, edit="J"),
    O_rec("QSYSPRT", "D", "F", cond=" 01", sa=2),
    O_fld("QTY", 10, edit="Z"),
    O_rec("QSYSPRT", "T", cond=" L1", sa=2),
    O_fld(const="'Region'", end=6),
    O_fld("REGION", 9),
    O_fld("REGTOT", 34, edit="J", blank="B"),
    O_rec("QSYSPRT", "T", cond=" LR"),
    O_fld(const="'Pages'", end=5),
    O_fld("PAGE", 12, edit="Z"),
]

M["CYC010"] = files("CUSTF") + [
    comment("Field indicators: plus, minus and zero on an external field."),
    F("CUSTF", "I", "P", fmt="E", rtype="K", dev="DISK"),
    PRT,
    I_xrec("CUSTR", "01"),
    I_xfld("BAL", plus="31", minus="32", zero="33"),
    I_xfld("CITY", level="L1"),
    O_rec("QSYSPRT", "D", cond=" 01 31", sa=1),
    O_fld("CUSNO", 5),
    O_fld(const="'credit'", end=13),
    O_rec("QSYSPRT", "D", cond=" 01 32", sa=1),
    O_fld("CUSNO", 5),
    O_fld(const="'debit'", end=12),
    O_rec("QSYSPRT", "D", cond=" 01 33", sa=1),
    O_fld("CUSNO", 5),
    O_fld(const="'zero'", end=11),
    O_rec("QSYSPRT", "D", cond=" L1", sa=1),
    O_fld(const="'city break'", end=10),
    O_fld("CITY", 23),
]

M["CYC011"] = files("MSTF", "ACTF") + [
    comment("Primary and secondary without matching: the primary, then the rest."),
    F("MSTF", "I", "P", fmt="F", rlen="13", dev="DISK"),
    F("ACTF", "I", "S", fmt="F", rlen="8", dev="DISK"),
    PRT,
    I_rec("MSTF", "NS", "01"),
    I_fld("MCUS", 1, 3),
    I_fld("MNAME", 4, 13),
    I_rec("ACTF", "NS", "02"),
    I_fld("ACUS", 1, 3),
    I_fld("AAMT", 4, 8, 0),
    C(cond=" 02", op="ADD", f2="AAMT", res="SUM", ln="7", dec="0"),
    O_rec("QSYSPRT", "D", cond=" 01", sa=1),
    O_fld(const="'M'", end=1),
    O_fld("MCUS", 5),
    O_fld("MNAME", 16),
    O_rec("QSYSPRT", "D", cond=" 02", sa=1),
    O_fld(const="'A'", end=1),
    O_fld("ACUS", 5),
    O_fld("AAMT", 12, edit="Z"),
    O_fld("SUM", 20, edit="Z"),
    O_rec("QSYSPRT", "T", cond=" LR"),
    O_fld(const="'sum'", end=3),
    O_fld("SUM", 12, edit="Z"),
]

M["CYC012"] = files("SALESF") + [
    comment("A classic report: headings, details, rep and region totals."),
    F("SALESF", "I", "P", fmt="E", dev="DISK"),
    PRT,
    D("EXT", "S", to="9", typ="P", dec="2"),
    D("REPTOT", "S", to="9", typ="P", dec="2"),
    D("REGTOT", "S", to="11", typ="P", dec="2"),
    D("GRAND", "S", to="11", typ="P", dec="2"),
    D("LINES", "S", to="3", typ="P", dec="0"),
    I_xrec("SALESR", "01"),
    I_xfld("REGION", level="L2"),
    I_xfld("REP", level="L1"),
    C(cond=" 01", f1="AMT", op="MULT", f2="QTY", res="EXT"),
    C(cond=" 01", op="ADD", f2="EXT", res="REPTOT"),
    C(cond=" 01", op="ADD", f2="1", res="LINES"),
    C(level="L1", op="ADD", f2="REPTOT", res="REGTOT"),
    C(level="L2", op="ADD", f2="REGTOT", res="GRAND"),
    O_rec("QSYSPRT", "H", cond=" 1P", kb=1, sa=1),
    O_or(" OF"),
    O_fld(const="'Sales by region and rep'", end=23),
    O_fld(const="'Page'", end=56),
    O_fld("PAGE", 61, edit="Z"),
    O_rec("QSYSPRT", "H", cond=" 1P", sa=2),
    O_or(" OF"),
    O_fld(const="'Reg Rep  Item'", end=14),
    O_fld(const="'Qty'", end=30),
    O_fld(const="'Price'", end=42),
    O_fld(const="'Extended'", end=56),
    O_rec("QSYSPRT", "D", cond=" 01 L1", sa=1),
    O_fld("REGION", 3),
    O_fld("REP", 8),
    O_rec("QSYSPRT", "D", cond=" 01", sa=1),
    O_fld("ITEM", 20),
    O_fld("QTY", 30, edit="3"),
    O_fld("AMT", 42, edit="1"),
    O_fld("EXT", 56, edit="1"),
    O_rec("QSYSPRT", "T", cond=" L1", sa=1),
    O_fld(const="'Rep'", end=14),
    O_fld("REP", 19),
    O_fld(const="'total'", end=25),
    O_fld("REPTOT", 56, edit="1", blank="B"),
    O_fld(const="'*'", end=57),
    O_rec("QSYSPRT", "T", cond=" L2", sa=2),
    O_fld(const="'Region'", end=11),
    O_fld("REGION", 14),
    O_fld(const="'total'", end=20),
    O_fld("REGTOT", 56, edit="1", blank="B"),
    O_fld(const="'**'", end=58),
    O_rec("QSYSPRT", "T", cond=" LR", sb=1),
    O_fld(const="'Grand total'", end=11),
    O_fld("LINES", 20, edit="Z"),
    O_fld(const="'lines'", end=26),
    O_fld("GRAND", 56, edit="1"),
    O_fld(const="'***'", end=59),
]


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    for name, lines in M.items():
        text = "\n".join(lines)
        for l in text.split("\n"):
            if len(l) > 80:
                raise SystemExit(f"{name}: too long: {l}")
        with open(os.path.join(OUT, name + ".rpgle"), "w", newline="\n") as f:
            f.write(text + "\n")
    print(len(M), "members")
