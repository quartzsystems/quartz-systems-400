"""Corpus batch 22: RPG III (RPG/400) programs. Writes ../b22rpg3/*.rpg.

Each member is compiled with CRTRPGPGM and run, on the oracle and in
the kit, and converted with CVTRPGSRC (docs/RPG3.md).

usage: python -I rpg3.py
"""
import os

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b22rpg3")
MEMBERS = {}


def place(parts, width=80):
    s = [" "] * width
    for col, text in parts:
        if text is None or text == "":
            continue
        for i, ch in enumerate(text):
            s[col - 1 + i] = ch
    return "".join(s).rstrip()


def rj(text, width):
    return "" if text == "" else str(text).rjust(width)


def ind(text):
    """An indicator entry of three columns: `N01`, `01`."""
    return "" if text == "" else text.rjust(3)


def star(text=""):
    t = "      *" + (" " + text if text else "")
    assert len(t) <= 74, t
    return t


def directive(text):
    return "      *; " + text


def H(**c):
    return place([(6, "H")] + [(int(k[1:]), v) for k, v in c.items()])


def F(name, typ, des="", eof="", seq="", fmt="F", rlen="", mode="", klen="", rat="", org="", ofl="",
      kloc="", ext="", dev="DISK", cont="", rtn="", entry="", add="", cond=""):
    return place([
        (6, "F"), (7, name), (15, typ), (16, des), (17, eof), (18, seq), (19, fmt),
        (24, rj(rlen, 4)), (28, mode), (29, rj(klen, 2)), (31, rat), (32, org), (33, ofl),
        (35, rj(kloc, 4)), (39, ext), (40, dev), (53, cont), (54, rtn), (60, entry), (66, add), (71, cond),
    ])


def FK(opt, entry="", at19="", at47=""):
    """A continuation line: K in 53, the option in 54-59."""
    return place([(6, "F"), (19, at19), (47, at47), (53, "K"), (54, opt), (60, entry)])


def E(name, n, length, frm="", to="", per="", fmt="", dec="", seq="", alt="", alen="", afmt="", adec="", aseq=""):
    return place([
        (6, "E"), (11, frm), (19, to), (27, name), (33, rj(per, 3)), (36, rj(n, 4)), (40, rj(length, 3)),
        (43, fmt), (44, dec), (45, seq), (46, alt), (52, rj(alen, 3)), (55, afmt), (56, adec), (57, aseq),
    ])


def Lspec(name, formlen, ofl):
    return place([(6, "L"), (7, name), (15, rj(formlen, 3)), (18, "FL"), (20, rj(ofl, 3)), (23, "OL")])


def IR(name, seq, ri, num="", opt="", codes=()):
    """A program-described record line; codes are (pos, not, czd, char)."""
    parts = [(6, "I"), (7, name), (15, seq), (17, num), (18, opt), (19, ri)]
    for k, (pos, nt, czd, ch) in enumerate(codes):
        base = 21 + 7 * k
        parts += [(base, rj(pos, 4)), (base + 4, nt), (base + 5, czd), (base + 6, ch)]
    return place(parts)


def IAND(codes, kind="AND"):
    parts = [(6, "I"), (14, kind)]
    for k, (pos, nt, czd, ch) in enumerate(codes):
        base = 21 + 7 * k
        parts += [(base, rj(pos, 4)), (base + 4, nt), (base + 5, czd), (base + 6, ch)]
    return place(parts)


def IF_(name, frm, to, fmt="", dec="", lvl="", match="", rel="", plus="", minus="", zero=""):
    return place([
        (6, "I"), (43, fmt), (44, rj(frm, 4)), (48, rj(to, 4)), (52, dec), (53, name), (59, lvl), (61, match),
        (63, rel), (65, plus), (67, minus), (69, zero),
    ])


def IDS(name="", ext="", opt="", extfile="", occurs="", length=""):
    return place([(6, "I"), (7, name), (17, ext), (18, opt), (19, "DS"), (21, extfile), (44, rj(occurs, 4)),
                  (48, rj(length, 4))])


def ISUB(name, frm="", to="", fmt="", dec="", init="", kw=""):
    """A subfield: from/to, or a special keyword in 44-51; `init` is the
    initial value in 21-42 (I in column 8)."""
    parts = [(6, "I"), (43, fmt), (52, dec), (53, name)]
    if init != "":
        parts += [(8, "I"), (21, init)]
    if kw:
        parts.append((44, kw))
    else:
        parts += [(44, rj(frm, 4)), (48, rj(to, 4))]
    return place(parts)


def ICON(name, value):
    return place([(6, "I"), (21, value), (43, "C"), (53, name)])


def IXR(fmt, ri=""):
    """An externally described record line."""
    return place([(6, "I"), (7, fmt), (19, ri)])


def IXF(ext, name="", lvl="", match="", plus="", minus="", zero=""):
    """An externally described field line: rename, level, matching."""
    return place([(6, "I"), (21, ext), (53, name), (59, lvl), (61, match), (65, plus), (67, minus), (69, zero)])


def C(op, f1="", f2="", res="", ln="", dec="", h="", hi="", lo="", eq="", lvl="", n1="", n2="", n3=""):
    return place([
        (6, "C"), (7, lvl), (9, ind(n1)), (12, ind(n2)), (15, ind(n3)), (18, f1), (28, op), (33, f2), (43, res),
        (49, rj(ln, 3)), (52, dec), (53, h), (54, hi), (56, lo), (58, eq),
    ])


def OR_(name, typ, sb="", sa="", kb="", ka="", n1="", n2="", n3="", excpt="", fetch="", adddel=""):
    return place([
        (6, "O"), (7, name), (15, typ), (16, fetch), (16, adddel), (17, sb), (18, sa), (19, rj(kb, 2)),
        (21, rj(ka, 2)), (23, ind(n1)), (26, ind(n2)), (29, ind(n3)), (32, excpt),
    ])


def OAND(kind="AND", n1="", n2="", n3=""):
    return place([(6, "O"), (14, kind), (23, ind(n1)), (26, ind(n2)), (29, ind(n3))])


def OF_(end, field="", edit="", blank="", fmt="", const="", n1="", n2="", n3=""):
    return place([
        (6, "O"), (23, ind(n1)), (26, ind(n2)), (29, ind(n3)), (32, field), (38, edit), (39, blank),
        (40, rj(end, 4)), (44, fmt), (45, const),
    ])


PRT = F("QSYSPRT", "O", fmt="F", rlen="132", ofl="OF", dev="PRINTER")


def member(name, lines):
    MEMBERS[name] = lines


def q(s):
    return "'" + s + "'"


# --- 1. Edit codes on positive, negative and zero values.
codes = ["1", "2", "3", "4", "A", "B", "C", "D", "J", "K", "L", "M", "N", "O", "P", "Q", "X", "Y", "Z"]
lines = [star("Edit codes: every code on a negative and a zero value."), H(), PRT,
         C("Z-ADD", f2="-1234.56", res="AMT", ln="7", dec="2"),
         C("Z-ADD", f2="0", res="ZRO", ln="7", dec="2"),
         C("Z-ADD", f2="70426", res="DAT", ln="6", dec="0"),
         C("EXCPT", f2="LINE"),
         C("SETON", hi="LR"),
         OR_("QSYSPRT", "E", sa="1", excpt="LINE")]
for i, e in enumerate(codes):
    if i and i % 4 == 0:
        lines.append(OR_("QSYSPRT", "E", sa="1", excpt="LINE"))
    k = i % 4
    lines.append(OF_(str(4 + 30 * k), const=q(e)))
    lines.append(OF_(str(18 + 30 * k), field="AMT", edit=e))
    lines.append(OF_(str(32 + 30 * k), field="ZRO", edit=e))
lines += [OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
          OF_("20", field="AMT", edit="J", const="'*'"),
          OF_("40", field="AMT", edit="1", const="'$'"),
          OF_("60", field="DAT", edit="Y"),
          OF_("80", field="ZRO", edit="1", const="'*'")]
member("R3001", lines)

# --- 2. Arithmetic operations, truncation, half adjust, indicators.
member("R3002", [
    star("Arithmetic: ADD SUB MULT DIV MVR Z-ADD Z-SUB SQRT, half adjust."), H(), PRT,
    C("Z-ADD", f2="100", res="A", ln="5", dec="0"),
    C("Z-ADD", f2="7", res="B", ln="3", dec="0"),
    C("DIV", f1="A", f2="B", res="Q", ln="7", dec="2", h="H"),
    C("DIV", f1="A", f2="B", res="Q0", ln="5", dec="0"),
    C("MVR", res="R", ln="3", dec="0"),
    C("MULT", f1="A", f2="1.5", res="M", ln="9", dec="2"),
    C("SUB", f1="B", f2="A", res="S", ln="5", dec="0", hi="10", lo="11", eq="12"),
    C("ADD", f1="A", f2="B", res="T", ln="5", dec="0"),
    C("ADD", f2="5", res="T"),
    C("SQRT", f2="A", res="SQ", ln="7", dec="4", h="H"),
    C("SQRT", f2="B", res="SQ2", ln="7", dec="4"),
    C("Z-SUB", f2="A", res="N", ln="5", dec="0"),
    C("Z-ADD", f2="99999", res="BIG", ln="5", dec="0"),
    C("ADD", f1="BIG", f2="1", res="OV", ln="5", dec="0", eq="13"),
    C("MULT", f1="BIG", f2="BIG", res="OV2", ln="5", dec="0"),
    C("Z-ADD", f2="2.555", res="HA", ln="5", dec="3"),
    C("Z-ADD", f2="HA", res="HB", ln="3", dec="2", h="H"),
    C("Z-ADD", f2="HA", res="HC", ln="3", dec="2"),
    C("MULT", f2="2", res="HA"),
    C("SUB", f2="5.11", res="HA", lo="14"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("10", field="Q", edit="J"),
    OF_("20", field="Q0", edit="J"),
    OF_("30", field="R", edit="J"),
    OF_("45", field="M", edit="J"),
    OF_("55", field="S", edit="J"),
    OF_("65", field="T", edit="J"),
    OF_("80", field="SQ", edit="J"),
    OF_("95", field="SQ2", edit="J"),
    OF_("105", field="N", edit="J"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("10", field="OV", edit="J"),
    OF_("20", field="OV2", edit="J"),
    OF_("30", field="HB", edit="J"),
    OF_("40", field="HC", edit="J"),
    OF_("50", field="HA", edit="J"),
    OF_("53", const="'10'", n1="10"),
    OF_("56", const="'11'", n1="11"),
    OF_("59", const="'12'", n1="12"),
    OF_("62", const="'13'", n1="13"),
    OF_("65", const="'14'", n1="14"),
])

# --- 3. Edit words.
member("R3003", [
    star("Edit words: separators, fill, floating currency, CR and minus."), H(), PRT,
    C("Z-ADD", f2="-123456.78", res="AMT", ln="9", dec="2"),
    C("Z-ADD", f2="4567.8", res="POS", ln="9", dec="2"),
    C("Z-ADD", f2="110426", res="DAT", ln="6", dec="0"),
    C("Z-ADD", f2="5551234", res="PHN", ln="7", dec="0"),
    C("Z-ADD", f2="0", res="ZRO", ln="9", dec="2"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("20", field="AMT", const="' ,   ,  0.  -'"),
    OF_("40", field="AMT", const="' ,   ,  0.  CR'"),
    OF_("60", field="POS", const="' ,   ,  0.  CR'"),
    OF_("80", field="POS", const="'*,   ,  0.  '"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("20", field="AMT", const="'$ ,   ,  0.  '"),
    OF_("40", field="DAT", const="'  /  /  '"),
    OF_("60", field="PHN", const="'   -    '"),
    OF_("80", field="ZRO", const="' ,   ,  0.  '"),
    OF_("100", field="ZRO", const="' ,   ,   .  '"),
])

# --- 4. Compare and branch: COMP, IFxx, DOW, DOU, CABxx, GOTO, CASxx, SELEC.
member("R3004", [
    star("Comparison and structured operations."), H(), PRT,
    C("Z-ADD", f2="5", res="X", ln="3", dec="0"),
    C("COMP", f1="X", f2="3", hi="10", lo="11", eq="12"),
    C("EXCPT", f2="CMP"),
    C("COMP", f1="X", f2="5", hi="10", lo="11", eq="12"),
    C("EXCPT", f2="CMP"),
    C("IFGT", f1="X", f2="4"),
    C("MOVE", f2="'GT4'", res="MSG", ln="10"),
    C("ELSE"),
    C("MOVE", f2="'LE4'", res="MSG"),
    C("END"),
    C("EXCPT", f2="MSGL"),
    C("Z-ADD", f2="0", res="I", ln="3", dec="0"),
    C("DOWLT", f1="I", f2="3"),
    C("ADD", f2="1", res="I"),
    C("EXCPT", f2="LOOP"),
    C("END"),
    C("DOUGE", f1="I", f2="5"),
    C("ADD", f2="1", res="I"),
    C("EXCPT", f2="LOOP"),
    C("ENDDO"),
    C("CABEQ", f1="I", f2="5", res="SKIP"),
    C("MOVEL", f2="'NOT SKIP'", res="MSG"),
    C("EXCPT", f2="MSGL"),
    C("TAG", f1="SKIP"),
    C("Z-ADD", f2="2", res="X"),
    C("CASEQ", f1="X", f2="1", res="SR1"),
    C("CASEQ", f1="X", f2="2", res="SR2"),
    C("CAS", res="SR3"),
    C("END"),
    C("SELEC"),
    C("WHEQ", f1="X", f2="1"),
    C("MOVEL", f2="'WHEN 1'", res="MSG"),
    C("WHGT", f1="X", f2="1"),
    C("ANDLT", f1="X", f2="9"),
    C("MOVEL", f2="'WHEN GT1'", res="MSG"),
    C("OTHER"),
    C("MOVEL", f2="'OTHER'", res="MSG"),
    C("ENDSL"),
    C("EXCPT", f2="MSGL"),
    C("IFEQ", f1="X", f2="7"),
    C("OREQ", f1="X", f2="2"),
    C("MOVEL", f2="'OR TRUE'", res="MSG"),
    C("EXCPT", f2="MSGL"),
    C("ENDIF"),
    C("GOTO", f2="DONE"),
    C("MOVEL", f2="'NOT HERE'", res="MSG"),
    C("EXCPT", f2="MSGL"),
    C("TAG", f1="DONE"),
    C("SETON", hi="LR"),
    C("BEGSR", f1="SR1"),
    C("MOVEL", f2="'SR1'", res="MSG"),
    C("EXCPT", f2="MSGL"),
    C("ENDSR"),
    C("BEGSR", f1="SR2"),
    C("MOVEL", f2="'SR2 RAN'", res="MSG"),
    C("EXCPT", f2="MSGL"),
    C("ENDSR"),
    C("BEGSR", f1="SR3"),
    C("MOVEL", f2="'SR3'", res="MSG"),
    C("EXCPT", f2="MSGL"),
    C("ENDSR"),
    OR_("QSYSPRT", "E", sa="1", excpt="CMP"),
    OF_("5", field="X", edit="Z"),
    OF_("10", const="'HI'", n1="10"),
    OF_("15", const="'LO'", n1="11"),
    OF_("20", const="'EQ'", n1="12"),
    OR_("QSYSPRT", "E", sa="1", excpt="MSGL"),
    OF_("12", field="MSG"),
    OR_("QSYSPRT", "E", sa="1", excpt="LOOP"),
    OF_("6", const="'LOOP'"),
    OF_("10", field="I", edit="Z"),
])

# --- 5. Moves.
member("R3005", [
    star("MOVE, MOVEL with and without padding, numeric and character."), H(), PRT,
    C("MOVE", f2="'ABCDEFGH'", res="C8", ln="8"),
    C("MOVE", f2="'XY'", res="C8"),
    C("MOVEL", f2="'XY'", res="C8B", ln="8"),
    C("MOVE", f2="'ABCDEFGH'", res="C8B"),
    C("MOVEL", f2="'XY'", res="C8B"),
    C("MOVE", f2="'ABCDEFGH'", res="C8C", ln="8"),
    C("MOVE", f2="'XY'", res="C8C", h="P"),
    C("MOVE", f2="'ABCDEFGH'", res="C8D", ln="8"),
    C("MOVEL", f2="'XY'", res="C8D", h="P"),
    C("Z-ADD", f2="-12345", res="N5", ln="5", dec="0"),
    C("MOVE", f2="N5", res="C3", ln="3"),
    C("MOVEL", f2="N5", res="C4", ln="4"),
    C("MOVE", f2="'987'", res="N5"),
    C("Z-ADD", f2="N5", res="N5B", ln="5", dec="0"),
    C("MOVEL", f2="'42'", res="N5B"),
    C("MOVE", f2="'1234567'", res="N3", ln="3", dec="0"),
    C("MOVE", f2="C8", res="C12", ln="12"),
    C("MOVEL", f2="C8", res="C12B", ln="12"),
    C("Z-ADD", f2="123.45", res="P52", ln="5", dec="2"),
    C("MOVE", f2="P52", res="C6", ln="6"),
    C("MOVE", f2="'J'", res="C1", ln="1"),
    C("MOVE", f2="C1", res="N1", ln="1", dec="0"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("10", field="C8"),
    OF_("20", field="C8B"),
    OF_("30", field="C8C"),
    OF_("40", field="C8D"),
    OF_("50", field="C3"),
    OF_("55", field="C4"),
    OF_("65", field="N5", edit="J"),
    OF_("75", field="N5B", edit="J"),
    OF_("85", field="N3", edit="J"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("14", field="C12"),
    OF_("28", field="C12B"),
    OF_("36", field="C6"),
    OF_("40", field="N1"),
])

# --- 6. Arrays and tables: compile-time data, LOKUP, XFOOT, SORTA, MOVEA.
member("R3006", [
    star("Arrays and tables from E specifications, with compile-time data."), H(), PRT,
    E("ARR", "8", "3", per="4", dec="0"),
    E("TABA", "4", "3", per="1", seq="A", alt="TABB", alen="10"),
    E("RUN", "5", "5", dec="2"),
    E("NAM", "3", "6", per="3"),
    C("XFOOT", f2="ARR", res="SUM", ln="7", dec="0"),
    C("Z-ADD", f2="1", res="I", ln="3", dec="0"),
    C("Z-ADD", f2="40", res="S1", ln="3", dec="0"),
    C("Z-ADD", f2="99", res="S2", ln="3", dec="0"),
    C("LOKUP", f1="S1", f2="ARR,I", eq="20"),
    C("Z-ADD", f2="1", res="J", ln="3", dec="0"),
    C("LOKUP", f1="S2", f2="ARR,J", eq="21"),
    C("LOKUP", f1="'B02'", f2="TABA", res="TABB", eq="22"),
    C("LOKUP", f1="'B05'", f2="TABA", res="TABB", hi="23"),
    C("Z-ADD", f2="1", res="K", ln="3", dec="0"),
    C("DO", f2="5", res="K"),
    C("Z-ADD", f2="K", res="RUN,K"),
    C("MULT", f2="1.5", res="RUN,K"),
    C("END"),
    C("ADD", f1="RUN,2", f2="RUN,5", res="TOT", ln="7", dec="2"),
    C("SORTA", f2="ARR"),
    C("MOVEA", f2="NAM", res="ALL", ln="18"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("10", field="SUM", edit="Z"),
    OF_("15", field="I", edit="Z"),
    OF_("20", field="J", edit="Z"),
    OF_("23", const="'20'", n1="20"),
    OF_("26", const="'21'", n1="21"),
    OF_("29", const="'22'", n1="22"),
    OF_("32", const="'23'", n1="23"),
    OF_("45", field="TABB"),
    OF_("50", field="TABA"),
    OF_("62", field="TOT", edit="J"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("40", field="ARR", edit="Z"),
    OF_("70", field="RUN", edit="J"),
    OF_("90", field="ALL"),
    "**",
    "050020040010",
    "070080030060",
    "**",
    "B01Alpha",
    "B02Beta",
    "B03Gamma",
    "B04Delta",
    "**",
    "ONE   TWO   THREE ",
])

# --- 7. String operations.
member("R3007", [
    star("CAT, CHECK, CHEKR, SCAN, SUBST, XLATE."), H(), PRT,
    ICON("DIG", "' 0123456789'"),
    C("MOVEL", f2="'HELLO'", res="A", ln="10"),
    C("MOVEL", f2="'WORLD'", res="B", ln="10"),
    C("CAT", f1="A", f2="B:1", res="R", ln="20"),
    C("CAT", f1="A", f2="B", res="R2", ln="20"),
    C("CAT", f1="'AB'", f2="'CD':2", res="R3", ln="10", h="P"),
    C("MOVEL", f2="'  12X4 '", res="S", ln="7"),
    C("CHECK", f1="DIG", f2="S", res="P1", ln="3", dec="0"),
    C("CHEKR", f1="' '", f2="S", res="P2", ln="3", dec="0"),
    C("SCAN", f1="'L'", f2="A", res="P3", ln="3", dec="0", eq="30"),
    C("SCAN", f1="'Q'", f2="A", res="P4", ln="3", dec="0", eq="31"),
    C("SCAN", f1="'L'", f2="A:4", res="P5", ln="3", dec="0"),
    C("SUBST", f1="3", f2="A:2", res="SB", ln="5", h="P"),
    C("XLATE", f1="'LO':'01'", f2="A", res="X", ln="10"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("20", field="R"),
    OF_("41", field="R2"),
    OF_("52", field="R3"),
    OF_("56", field="P1", edit="Z"),
    OF_("60", field="P2", edit="Z"),
    OF_("64", field="P3", edit="Z"),
    OF_("68", field="P4", edit="Z"),
    OF_("72", field="P5", edit="Z"),
    OF_("75", const="'30'", n1="30"),
    OF_("78", const="'31'", n1="31"),
    OF_("84", field="SB"),
    OF_("95", field="X"),
])

# --- 8. Data structures: subfields, overlay, initial values, constants, OCUR.
member("R3008", [
    star("Data structures from I specifications."), H(), PRT,
    IDS("DS1"),
    ISUB("PART1", "1", "6"),
    ISUB("PART2", "7", "10"),
    ISUB("WHOLE", "1", "10"),
    ISUB("NUM", "11", "15", dec="2"),
    ISUB("PNUM", "16", "18", fmt="P", dec="0"),
    ISUB("BNUM", "19", "22", fmt="B", dec="0"),
    IDS("DS2", opt="I"),
    ISUB("SUBA", "1", "3", init="'ABC'"),
    ISUB("SUBN", "4", "8", dec="0", init="123"),
    ISUB("SUBC", "9", "12"),
    IDS("MO", occurs="3", length="8"),
    ISUB("MOA", "1", "4"),
    ISUB("MOB", "5", "8", dec="0"),
    ICON("CON1", "'A CONSTANT'"),
    ICON("CON2", "1234.5"),
    C("MOVEL", f2="'FIRSTSEC'", res="WHOLE"),
    C("Z-ADD", f2="-12.34", res="NUM"),
    C("Z-ADD", f2="789", res="PNUM"),
    C("Z-ADD", f2="-5", res="BNUM"),
    C("Z-ADD", f2="CON2", res="CN", ln="7", dec="2"),
    C("Z-ADD", f2="1", res="O", ln="3", dec="0"),
    C("DO", f2="3", res="O"),
    C("OCUR", f1="O", f2="MO"),
    C("MOVEL", f2="'OC'", res="MOA"),
    C("MOVE", f2="O", res="MOA"),
    C("Z-ADD", f2="O", res="MOB"),
    C("MULT", f2="11", res="MOB"),
    C("END"),
    C("OCUR", f1="2", f2="MO"),
    C("MOVEL", f2="MO", res="MO2", ln="8"),
    C("OCUR", f2="MO", res="CUR", ln="3", dec="0"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("10", field="PART1"),
    OF_("16", field="PART2"),
    OF_("30", field="NUM", edit="J"),
    OF_("40", field="PNUM", edit="J"),
    OF_("50", field="BNUM", edit="J"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("12", field="DS2"),
    OF_("25", field="CON1"),
    OF_("35", field="CN", edit="J"),
    OF_("45", field="MO2"),
    OF_("50", field="CUR", edit="Z"),
    OF_("60", field="SUBN", edit="J"),
])

# --- 9. Program status data structure.
member("R3009", [
    star("The program status data structure and *ENTRY parameters."), H(), PRT,
    IDS("PSDS", opt="S"),
    ISUB("PGMNAM", kw="*PROGRAM"),
    ISUB("STS", kw="*STATUS"),
    ISUB("PRMS", kw="*PARMS"),
    ISUB("RTN", kw="*ROUTINE"),
    C("PLIST", f1="*ENTRY"),
    C("PARM", res="P1", ln="3"),
    C("PARM", res="P2", ln="15", dec="5"),
    C("ADD", f2="1", res="P2"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("10", field="PGMNAM"),
    OF_("20", field="STS"),
    OF_("25", field="PRMS", edit="Z"),
    OF_("35", field="RTN"),
    OF_("40", field="P1"),
    OF_("65", field="P2", edit="J"),
])
MEMBERS["R3009"].insert(0, directive("call: CALL PGM({lib}/{name}) PARM('ABC' 41.5)"))

# --- 10. Indicators.
member("R3010", [
    star("Indicators: SETON, SETOF, *IN, *INxx, AND and OR conditioning."), H(), PRT,
    C("SETON", hi="01", lo="02", eq="03"),
    C("SETOF", hi="02"),
    C("MOVE", f2="*IN03", res="SAVE", ln="1"),
    C("MOVE", f2="'1'", res="*IN04"),
    C("MOVEA", f2="'0110'", res="*IN,10"),
    C("Z-ADD", f2="20", res="X", ln="3", dec="0"),
    C("MOVE", f2="'1'", res="*IN,X"),
    C("SETON", hi="50", n1="01", n2="N02"),
    C("SETON", hi="51", n1="02"),
    C("SETON", hi="52", lvl="", n1="01"),
    C("MOVEA", f2="*IN,10", res="FOUR", ln="4"),
    C("MOVEA", f2="*IN", res="ALL", ln="25"),
    C("EXCPT", f2="LINE"),
    C("SETOF", hi="01", lo="03", eq="04"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("3", const="'01'", n1="01"),
    OF_("6", const="'02'", n1="02"),
    OF_("9", const="'03'", n1="03"),
    OF_("12", const="'04'", n1="04"),
    OF_("15", const="'50'", n1="50"),
    OF_("18", const="'51'", n1="51"),
    OF_("21", const="'52'", n1="52"),
    OF_("24", const="'NR'", n1="N01", n2="N03"),
    OF_("27", const="'OR'", n1="01", n2="03"),
    OF_("30", field="SAVE"),
    OF_("36", field="FOUR"),
    OF_("62", field="ALL"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE", n1="N01"),
    OAND("AND", n1="N03"),
    OF_("20", const="'BOTH OFF'"),
])

# --- 11. HLINV-style report: level breaks over a program-described primary file.
member("R3011", [
    directive("dds: SALESF"),
    directive("sqlfile: SALESF"),
    star("Sales report: L2 region, L1 representative, totals at each level."), H(),
    F("SALESF", "I", des="P", fmt="F", rlen="22", dev="DISK"),
    F("QSYSPRT", "O", fmt="F", rlen="132", ofl="OF", dev="PRINTER"),
    IR("SALESF", "NS", "01"),
    IF_("REGION", "1", "2", lvl="L2"),
    IF_("REP", "3", "6", lvl="L1"),
    IF_("ITEM", "7", "16"),
    IF_("AMT", "17", "20", fmt="P", dec="2"),
    IF_("QTY", "21", "22", fmt="P", dec="0"),
    C("MULT", f1="AMT", f2="QTY", res="EXT", ln="9", dec="2", n1="01"),
    C("ADD", f2="EXT", res="REPTOT", ln="11", dec="2", n1="01"),
    C("ADD", f2="1", res="CNT", ln="5", dec="0", n1="01"),
    C("ADD", f2="REPTOT", res="REGTOT", ln="11", dec="2", lvl="L1"),
    C("ADD", f2="REGTOT", res="GRAND", ln="11", dec="2", lvl="L2"),
    OR_("QSYSPRT", "H", sb="", sa="2", kb="1", n1="1P"),
    OAND("OR", n1="OF"),
    OF_("20", const="'SALES BY REGION'"),
    OF_("40", const="'PAGE'"),
    OF_("45", field="PAGE", edit="Z"),
    OR_("QSYSPRT", "D", sa="1", n1="01"),
    OF_("4", field="REGION", n1="L2"),
    OF_("10", field="REP", n1="L1"),
    OF_("22", field="ITEM"),
    OF_("34", field="AMT", edit="J"),
    OF_("40", field="QTY", edit="Z"),
    OF_("55", field="EXT", edit="J"),
    OR_("QSYSPRT", "T", sa="1", n1="L1"),
    OF_("22", const="'REP TOTAL'"),
    OF_("10", field="REP"),
    OF_("70", field="REPTOT", edit="J", blank="B"),
    OR_("QSYSPRT", "T", sa="2", n1="L2"),
    OF_("22", const="'REGION TOTAL'"),
    OF_("4", field="REGION"),
    OF_("85", field="REGTOT", edit="J", blank="B"),
    OR_("QSYSPRT", "T", sb="1", n1="LR"),
    OF_("22", const="'GRAND TOTAL'"),
    OF_("30", field="CNT", edit="Z"),
    OF_("100", field="GRAND", edit="J"),
])

# --- 12. Matching records.
member("R3012", [
    directive("dds: MSTF"),
    directive("sqlfile: MSTF"),
    directive("dds: ACTF"),
    directive("sqlfile: ACTF"),
    star("Matching records: masters primary, activity secondary, M1 and MR."), H(),
    F("MSTF", "I", des="P", fmt="F", rlen="13"),
    F("ACTF", "I", des="S", fmt="F", rlen="8"),
    PRT,
    IR("MSTF", "AA", "01"),
    IF_("MCUS", "1", "3", match="M1"),
    IF_("MNAME", "4", "13"),
    IR("ACTF", "BB", "02"),
    IF_("ACUS", "1", "3", match="M1"),
    IF_("AAMT", "4", "8", dec="0"),
    C("ADD", f2="AAMT", res="TOT", ln="7", dec="0", n1="02", n2="MR"),
    C("ADD", f2="1", res="UNM", ln="3", dec="0", n1="02", n2="NMR"),
    OR_("QSYSPRT", "D", sa="1", n1="01", n2="MR"),
    OF_("3", field="MCUS"),
    OF_("15", field="MNAME"),
    OF_("37", const="'master with activity'"),
    OR_("QSYSPRT", "D", sa="1", n1="01", n2="NMR"),
    OF_("3", field="MCUS"),
    OF_("15", field="MNAME"),
    OF_("29", const="'master alone'"),
    OR_("QSYSPRT", "D", sa="1", n1="02", n2="MR"),
    OF_("7", field="ACUS"),
    OF_("15", field="AAMT", edit="Z"),
    OF_("24", const="'matched'"),
    OR_("QSYSPRT", "D", sa="1", n1="02", n2="NMR"),
    OF_("7", field="ACUS"),
    OF_("15", field="AAMT", edit="Z"),
    OF_("26", const="'unmatched'"),
    OR_("QSYSPRT", "T", sb="1", n1="LR"),
    OF_("10", field="TOT", edit="Z"),
    OF_("15", field="UNM", edit="Z"),
])

# --- 13. Externally described input: READ, CHAIN, SETLL, READE.
member("R3013", [
    directive("dds: CUSTF"),
    directive("sqlfile: CUSTF"),
    star("An externally described keyed file: READ, CHAIN, SETLL, READE."), H(),
    F("CUSTF", "I", des="F", fmt="E", rat="K", dev="DISK"),
    PRT,
    C("READ", f2="CUSTF", eq="90"),
    C("DOWEQ", f1="*IN90", f2="'0'"),
    C("EXCPT", f2="LINE"),
    C("READ", f2="CUSTF", eq="90"),
    C("END"),
    C("Z-ADD", f2="104", res="KEY", ln="5", dec="0"),
    C("CHAIN", f1="KEY", f2="CUSTF", hi="91"),
    C("EXCPT", f2="LINE", n1="N91"),
    C("Z-ADD", f2="999", res="K2", ln="5", dec="0"),
    C("CHAIN", f1="K2", f2="CUSTF", hi="92"),
    C("EXCPT", f2="NF", n1="92"),
    C("Z-ADD", f2="106", res="K3", ln="5", dec="0"),
    C("SETLL", f1="K3", f2="CUSTF", eq="93"),
    C("READ", f2="CUSTF", eq="94"),
    C("EXCPT", f2="LINE", n1="N94"),
    C("READP", f2="CUSTF", eq="95"),
    C("READP", f2="CUSTF", eq="95"),
    C("EXCPT", f2="LINE", n1="N95"),
    C("Z-ADD", f2="102", res="K4", ln="5", dec="0"),
    C("SETLL", f1="K4", f2="CUSTF", eq="96"),
    C("READE", f1="K4", f2="CUSTF", eq="97"),
    C("EXCPT", f2="LINE", n1="N97"),
    C("READE", f1="K4", f2="CUSTF", eq="97"),
    C("EXCPT", f2="NF", n1="97"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("6", field="CUSNO", edit="Z"),
    OF_("28", field="NAME"),
    OF_("41", field="CITY"),
    OF_("55", field="BAL", edit="J"),
    OR_("QSYSPRT", "E", sa="1", excpt="NF"),
    OF_("10", const="'NOT FOUND'"),
    OF_("13", const="'93'", n1="93"),
])

# --- 14. Update, add, delete.
member("R3014", [
    directive("dds: CUSTF"),
    directive("sqlfile: CUSTF"),
    star("Update, write and delete through an externally described file."), H(),
    F("CUSTF", "U", des="F", fmt="E", rat="K", dev="DISK", add="A"),
    PRT,
    C("Z-ADD", f2="102", res="K", ln="5", dec="0"),
    C("CHAIN", f1="K", f2="CUSTF", hi="91"),
    C("ADD", f2="100", res="BAL", n1="N91"),
    C("UPDAT", f2="CUSTR", n1="N91"),
    C("Z-ADD", f2="109", res="CUSNO"),
    C("MOVEL", f2="'Ida Ives'", res="NAME", h="P"),
    C("MOVEL", f2="'Erie'", res="CITY", h="P"),
    C("Z-ADD", f2="7.77", res="BAL"),
    C("WRITE", f2="CUSTR", lo="93"),
    C("WRITE", f2="CUSTR", lo="94"),
    C("Z-ADD", f2="103", res="K"),
    C("CHAIN", f1="K", f2="CUSTF", hi="91"),
    C("DELET", f2="CUSTR", n1="N91"),
    C("Z-ADD", f2="105", res="K"),
    C("CHAIN", f1="K", f2="CUSTF", hi="91"),
    C("SETLL", f1="*LOVAL", f2="CUSTF"),
    C("READ", f2="CUSTR", eq="90"),
    C("DOWEQ", f1="*IN90", f2="'0'"),
    C("EXCPT", f2="LINE"),
    C("READ", f2="CUSTR", eq="90"),
    C("END"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("6", field="CUSNO", edit="Z"),
    OF_("28", field="NAME"),
    OF_("55", field="BAL", edit="J"),
    OF_("58", const="'93'", n1="93"),
    OF_("61", const="'94'", n1="94"),
])

# --- 15. Subroutines, *INZSR, DO loops with ITER and LEAVE.
member("R3015", [
    star("Subroutines, *INZSR, DO with a step, ITER and LEAVE."), H(), PRT,
    C("DO", f1="1", f2="10", res="I", ln="3", dec="0"),
    C("DIV", f1="I", f2="2", res="H", ln="3", dec="0"),
    C("MVR", res="RM", ln="1", dec="0"),
    C("IFEQ", f1="RM", f2="0"),
    C("ITER"),
    C("END"),
    C("IFGT", f1="I", f2="7"),
    C("LEAVE"),
    C("END"),
    C("EXSR", f2="SHOW"),
    C("END"),
    C("DO", f1="2", f2="LIM", res="J", ln="3", dec="0"),
    C("EXCPT", f2="JL"),
    C("ENDDO", f2="3"),
    C("SETON", hi="LR"),
    C("BEGSR", f1="*INZSR"),
    C("Z-ADD", f2="11", res="LIM", ln="3", dec="0"),
    C("Z-ADD", f2="0", res="CNT", ln="3", dec="0"),
    C("ENDSR"),
    C("BEGSR", f1="SHOW"),
    C("ADD", f2="1", res="CNT"),
    C("EXCPT", f2="IL"),
    C("ENDSR"),
    OR_("QSYSPRT", "E", sa="1", excpt="IL"),
    OF_("5", field="I", edit="Z"),
    OF_("10", field="CNT", edit="Z"),
    OR_("QSYSPRT", "E", sa="1", excpt="JL"),
    OF_("8", const="'J'"),
    OF_("12", field="J", edit="Z"),
])

# --- 16. Printer control: L specification, overflow, fetch overflow, skips.
member("R3016", [
    star("Form length 20, overflow line 15, headings again on overflow."), H(),
    F("QSYSPRT", "O", fmt="F", rlen="80", ofl="OF", ext="L", dev="PRINTER"),
    Lspec("QSYSPRT", "20", "15"),
    C("EXCPT", f2="HEAD"),
    C("DO", f1="1", f2="25", res="I", ln="3", dec="0"),
    C("EXCPT", f2="DET"),
    C("EXCPT", f2="HEAD", n1="OF"),
    C("SETOF", hi="OF", n1="OF"),
    C("END"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", kb="2", sa="2", excpt="HEAD"),
    OF_("20", const="'HEADING'"),
    OF_("30", field="PAGE", edit="Z"),
    OR_("QSYSPRT", "E", sa="1", excpt="DET"),
    OF_("8", const="'LINE'"),
    OF_("12", field="I", edit="Z"),
    OR_("QSYSPRT", "T", sb="2", n1="LR"),
    OF_("12", const="'END'"),
])

# --- 17. Record identification codes on a program-described primary file.
member("R3017", [
    directive("dds: TRANF"),
    directive("sqlfile: TRANF"),
    star("Record types by identification codes; others are skipped."), H(),
    F("TRANF", "I", des="P", fmt="F", rlen="20"),
    PRT,
    IR("TRANF", "01", "10", num="1", codes=[("1", "", "C", "H")]),
    IF_("CUST", "2", "4", dec="0"),
    IF_("CNAME", "5", "20"),
    IR("TRANF", "02", "20", num="N", codes=[("1", "", "C", "D")]),
    IF_("AMT", "2", "8", dec="2"),
    IF_("ITEM", "9", "20"),
    IR("TRANF", "NS", "30"),
    C("ADD", f2="AMT", res="TOT", ln="9", dec="2", n1="20"),
    C("ADD", f2="1", res="OTH", ln="3", dec="0", n1="30"),
    OR_("QSYSPRT", "D", sa="1", n1="10"),
    OF_("5", field="CUST"),
    OF_("22", field="CNAME"),
    OR_("QSYSPRT", "D", sa="1", n1="20"),
    OF_("20", field="ITEM"),
    OF_("35", field="AMT", edit="J"),
    OR_("QSYSPRT", "T", sa="1", n1="LR"),
    OF_("35", field="TOT", edit="J"),
    OF_("40", field="OTH", edit="Z"),
])

# --- 18. KLIST and a two-field key.
member("R3018", [
    directive("dds: ORDF"),
    directive("sqlfile: ORDF"),
    star("KLIST and KFLD: CHAIN, READE by a partial key, SETGT, READP."), H(),
    F("ORDF", "I", des="F", fmt="E", rat="K"),
    PRT,
    C("KLIST", f1="FULL"),
    C("KFLD", res="KCUS"),
    C("KFLD", res="KORD"),
    C("KLIST", f1="PART"),
    C("KFLD", res="KCUS"),
    C("Z-ADD", f2="101", res="KCUS", ln="5", dec="0"),
    C("Z-ADD", f2="2", res="KORD", ln="5", dec="0"),
    C("CHAIN", f1="FULL", f2="ORDF", hi="91"),
    C("EXCPT", f2="LINE", n1="N91"),
    C("SETLL", f1="PART", f2="ORDF"),
    C("READE", f1="PART", f2="ORDF", eq="92"),
    C("DOWEQ", f1="*IN92", f2="'0'"),
    C("EXCPT", f2="LINE"),
    C("READE", f1="PART", f2="ORDF", eq="92"),
    C("END"),
    C("Z-ADD", f2="104", res="KCUS"),
    C("SETGT", f1="PART", f2="ORDF"),
    C("READP", f2="ORDF", eq="93"),
    C("EXCPT", f2="LINE", n1="N93"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("6", field="ORDCUS", edit="Z"),
    OF_("12", field="ORDNO", edit="Z"),
    OF_("24", field="ITEM"),
    OF_("30", field="QTY", edit="Z"),
    OF_("40", field="PRICE", edit="J"),
])

# --- 19. Externally described data structure and field renames.
member("R3019", [
    directive("dds: CUSTF"),
    directive("sqlfile: CUSTF"),
    star("An externally described data structure; a renamed input field."), H(),
    F("CUSTF", "I", des="F", fmt="E", rat="K"),
    PRT,
    IXR("CUSTR"),
    IXF("NAME", "CNAME"),
    IDS("CUSTDS", ext="E", extfile="CUSTF"),
    IXF("CITY", "DSCITY"),
    C("Z-ADD", f2="101", res="K", ln="5", dec="0"),
    C("CHAIN", f1="K", f2="CUSTF", hi="91"),
    C("MOVEL", f2="CNAME", res="SAVNAM", ln="20"),
    C("MOVEL", f2="'Lima'", res="DSCITY", h="P"),
    C("Z-ADD", f2="5", res="CUSNO"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("20", field="SAVNAM"),
    OF_("30", field="CITY"),
    OF_("70", field="CUSTDS"),
])

# --- 20. Bit operations and numeric tests.
member("R3020", [
    star("BITON, BITOF, TESTB, TESTN, TESTZ, MHHZO, MLLZO, MHLZO, MLHZO."), H(), PRT,
    C("MOVE", f2="X'00'", res="B", ln="1"),
    C("BITON", f2="'0246'", res="B"),
    C("BITOF", f2="'2'", res="B"),
    C("TESTB", f2="'0'", res="B", hi="10", lo="11", eq="12"),
    C("TESTB", f2="'0123'", res="B", hi="13", lo="14", eq="15"),
    C("MOVE", f2="'  123'", res="T1", ln="5"),
    C("TESTN", res="T1", hi="20", lo="21", eq="22"),
    C("MOVE", f2="'12A45'", res="T2", ln="5"),
    C("TESTN", res="T2", hi="23", lo="24", eq="25"),
    C("MOVE", f2="'J'", res="Z1", ln="1"),
    C("TESTZ", res="Z1", hi="30", lo="31", eq="32"),
    C("MOVE", f2="'A1B2'", res="M", ln="4"),
    C("MHHZO", f2="'J'", res="M"),
    C("MOVE", f2="'A1B2'", res="M2", ln="4"),
    C("MLLZO", f2="'J'", res="M2"),
    C("BITON", f2="'7'", res="B"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("3", const="'10'", n1="10"),
    OF_("6", const="'11'", n1="11"),
    OF_("9", const="'12'", n1="12"),
    OF_("12", const="'13'", n1="13"),
    OF_("15", const="'14'", n1="14"),
    OF_("18", const="'15'", n1="15"),
    OF_("21", const="'20'", n1="20"),
    OF_("24", const="'21'", n1="21"),
    OF_("27", const="'22'", n1="22"),
    OF_("30", const="'23'", n1="23"),
    OF_("33", const="'24'", n1="24"),
    OF_("36", const="'25'", n1="25"),
    OF_("39", const="'30'", n1="30"),
    OF_("42", const="'31'", n1="31"),
    OF_("45", const="'32'", n1="32"),
    OF_("50", field="M"),
    OF_("55", field="M2"),
])

# --- 21. File continuation lines: RENAME, INFDS; a file information data structure.
member("R3021", [
    directive("dds: CUSTF"),
    directive("sqlfile: CUSTF"),
    star("Continuation lines: RENAME and INFDS; the INFDS's file and status."), H(),
    F("CUSTF", "I", des="F", fmt="E", rat="K"),
    FK("RENAME", "CREC", at19="CUSTR"),
    FK("INFDS", "FEED"),
    PRT,
    IDS("FEED"),
    ISUB("FFILE", kw="*FILE"),
    ISUB("FSTS", kw="*STATUS"),
    ISUB("FOPC", kw="*OPCODE"),
    ISUB("FREC", kw="*RECORD"),
    C("Z-ADD", f2="107", res="K", ln="5", dec="0"),
    C("CHAIN", f1="K", f2="CREC", hi="91"),
    C("EXCPT", f2="LINE"),
    C("Z-ADD", f2="999", res="K"),
    C("CHAIN", f1="K", f2="CUSTF", hi="91"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("10", field="FFILE"),
    OF_("20", field="FSTS"),
    OF_("30", field="FOPC"),
    OF_("40", field="FREC"),
    OF_("60", field="NAME"),
    OF_("63", const="'91'", n1="91"),
])

# --- 22. Three control levels over an externally described primary file.
member("R3022", [
    directive("dds: ORDF"),
    directive("sqlfile: ORDF"),
    star("Externally described primary file with an L1 break and LR totals."), H(),
    F("ORDF", "I", des="P", fmt="E", dev="DISK"),
    PRT,
    IXR("ORDR", "01"),
    IXF("ORDCUS", lvl="L1"),
    C("MULT", f1="QTY", f2="PRICE", res="EXT", ln="9", dec="2", n1="01"),
    C("ADD", f2="EXT", res="CUSTOT", ln="9", dec="2", n1="01"),
    C("ADD", f2="CUSTOT", res="GRAND", ln="11", dec="2", lvl="L1"),
    C("ADD", f2="1", res="NCUS", ln="3", dec="0", lvl="L1"),
    OR_("QSYSPRT", "H", sa="1", n1="1P"),
    OF_("30", const="'ORDERS BY CUSTOMER'"),
    OR_("QSYSPRT", "D", sa="1", n1="01"),
    OF_("6", field="ORDCUS", edit="Z", n1="L1"),
    OF_("12", field="ORDNO", edit="Z"),
    OF_("24", field="ITEM"),
    OF_("40", field="EXT", edit="J"),
    OR_("QSYSPRT", "T", sa="1", n1="L1"),
    OF_("24", const="'CUSTOMER TOTAL'"),
    OF_("50", field="CUSTOT", edit="J", blank="B"),
    OR_("QSYSPRT", "T", sa="1", n1="LR"),
    OF_("24", const="'ALL CUSTOMERS'"),
    OF_("30", field="NCUS", edit="Z"),
    OF_("60", field="GRAND", edit="J"),
])

# --- 23. *LIKE DEFN, a data area, and the local data area.
member("R3023", [
    directive("setup: CRTDTAARA DTAARA({lib}/R3DTA) TYPE(*CHAR) LEN(10) VALUE('START')"),
    directive("cleanup: DLTDTAARA DTAARA({lib}/R3DTA)"),
    star("DEFN: *LIKE and *NAMVAR; IN and OUT of a data area."), H(), PRT,
    C("Z-ADD", f2="12.5", res="BASE", ln="5", dec="1"),
    C("DEFN", f1="*LIKE", f2="BASE", res="COPY", ln="+2"),
    C("Z-ADD", f2="1234.5", res="COPY"),
    C("DEFN", f1="*NAMVAR", f2="R3DTA", res="DTA", ln="10"),
    C("IN", f1="*LOCK", f2="DTA"),
    C("MOVE", f2="DTA", res="BEFORE", ln="10"),
    C("MOVEL", f2="'CHANGED'", res="DTA"),
    C("OUT", f2="DTA"),
    C("IN", f2="DTA"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("10", field="COPY", edit="J"),
    OF_("25", field="BEFORE"),
    OF_("40", field="DTA"),
])

# --- 24. A compile error: an undefined field and a bad operation.
member("R3024", [
    directive("run: no"),
    star("Errors: a field never defined, an operation that does not exist."), H(), PRT,
    C("ADD", f1="NOPE", f2="1", res="X", ln="3", dec="0"),
    C("FOO", f2="X"),
    C("SETON", hi="LR"),
])

# --- 25. Numeric data in character fields; MOVE between types; signs.
member("R3025", [
    star("Signs and zones: MOVE of negative numbers, zoned results."), H(), PRT,
    C("Z-ADD", f2="-7", res="N2", ln="2", dec="0"),
    C("MOVE", f2="N2", res="C2", ln="2"),
    C("MOVE", f2="'1P'", res="N2B", ln="2", dec="0"),
    C("Z-ADD", f2="N2B", res="P3", ln="3", dec="0"),
    C("MOVE", f2="-12.5", res="C5", ln="5"),
    C("Z-ADD", f2="0", res="Z5", ln="5", dec="2"),
    C("SUB", f2="0.01", res="Z5"),
    C("MULT", f2="-1", res="Z5"),
    C("EXCPT", f2="LINE"),
    C("SETON", hi="LR"),
    OR_("QSYSPRT", "E", sa="1", excpt="LINE"),
    OF_("5", field="C2"),
    OF_("15", field="N2B", edit="J"),
    OF_("25", field="P3", edit="J"),
    OF_("35", field="C5"),
    OF_("45", field="Z5", edit="J"),
    OF_("55", field="Z5"),
])

if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    for name, lines in MEMBERS.items():
        for i, l in enumerate(lines):
            assert len(l) <= 80, (name, i, l)
        with open(os.path.join(OUT, name + ".rpg"), "w", newline="\n") as f:
            f.write("\n".join(lines) + "\n")
    print(len(MEMBERS), "members")
