"""Column placement for fixed-form RPG IV specifications.

Each helper returns one source record with its entries at IBM's columns
(1-based, inclusive); the corpus generators build members from them.
"""


def place(parts, width=80):
    """parts: list of (column, text); returns the record, right-trimmed."""
    s = [" "] * width
    for col, text in parts:
        if text is None or text == "":
            continue
        for i, ch in enumerate(text):
            if col - 1 + i >= len(s):
                s.extend([" "] * (col + i - len(s)))
            s[col - 1 + i] = ch
    return "".join(s).rstrip()


def rj(text, width):
    """Right-justify a numeric entry in its field."""
    return str(text).rjust(width)


def H(kw):
    return place([(6, "H"), (7, kw)])


def F(name, ftype, desig="", eof="", add="", seq="", fmt="F", rlen="", lim="", klen="", rtype="", org="", dev="DISK", kw=""):
    return place([
        (6, "F"), (7, name), (17, ftype), (18, desig), (19, eof), (20, add), (21, seq), (22, fmt),
        (23, rj(rlen, 5) if rlen != "" else ""), (28, lim), (29, rj(klen, 5) if klen != "" else ""),
        (34, rtype), (35, org), (36, dev), (44, kw),
    ])


def D(name="", deftype="", frm="", to="", typ="", dec="", kw="", ext="", dstype=""):
    return place([
        (6, "D"), (7, name), (22, ext), (23, dstype), (24, deftype),
        (26, rj(frm, 7) if frm != "" else ""), (33, rj(to, 7) if to != "" else ""),
        (40, typ), (41, rj(dec, 2) if dec != "" else ""), (44, kw),
    ])


def C(f1="", op="", f2="", res="", ln="", dec="", hi="", lo="", eq="", level="", cond=""):
    return place([
        (6, "C"), (7, level), (9, cond), (12, f1), (26, op), (36, f2), (50, res),
        (64, rj(ln, 5) if ln != "" else ""), (69, rj(dec, 2) if dec != "" else ""),
        (71, hi), (73, lo), (75, eq),
    ])


def wrap(text, width=45):
    """Cut an extended factor 2 at blanks outside literals into pieces
    of at most `width` characters (columns 36 to 80)."""
    pieces, cur, quote = [], "", False
    words, w = [], ""
    for ch in text:
        if ch == "'":
            quote = not quote
        if ch == " " and not quote:
            words.append(w)
            w = ""
        else:
            w += ch
    words.append(w)
    for word in words:
        cand = word if not cur else cur + " " + word
        if len(cand) > width and cur:
            pieces.append(cur)
            cur = word
        else:
            cur = cand
    pieces.append(cur)
    return pieces


def CX(op, xf2="", f1="", level="", cond=""):
    parts = wrap(xf2)
    first = place([(6, "C"), (7, level), (9, cond), (12, f1), (26, op), (36, parts[0])])
    rest = [place([(6, "C"), (36, p)]) for p in parts[1:]]
    return "\n".join([first] + rest)


def P(name, begin, kw=""):
    return place([(6, "P"), (7, name), (24, begin), (44, kw)])


def O_rec(name, typ, fetch="", cond="", exc="", sb="", sa="", kb="", ka=""):
    return place([
        (6, "O"), (7, name), (17, typ), (18, fetch), (21, cond), (30, exc),
        (40, rj(sb, 3) if sb != "" else ""), (43, rj(sa, 3) if sa != "" else ""),
        (46, rj(kb, 3) if kb != "" else ""), (49, rj(ka, 3) if ka != "" else ""),
    ])


def O_fld(field="", end="", edit="", blank="", const="", cond="", fmt=""):
    return place([
        (6, "O"), (21, cond), (30, field), (44, edit), (45, blank),
        (47, rj(end, 5) if end != "" else ""), (52, fmt), (53, const),
    ])


def comment(text):
    return place([(7, "*"), (8, " " + text)])


def free(text):
    """A free-form line in fixed-format source: columns 8 to 80."""
    return place([(8, text)])


# The print idiom of fixed-form members: QSYSPRT and a 132-byte line.
PRT_F = F("QSYSPRT", "O", fmt="F", rlen="132", dev="PRINTER")
PRT_D = D("LINE", "DS", to="132")


def show(expr):
    """Print an expression (character) on QSYSPRT."""
    return [CX("EVAL", "LINE = " + expr), C(op="WRITE", f2="QSYSPRT", res="LINE")]


def I_rec(name, seq, rid="", codes=(), number="", option=""):
    """A program-described record line; codes are (pos, not, part, char)."""
    parts = [(6, "I"), (7, name), (17, seq), (19, number), (20, option), (21, rid)]
    for k, (pos, neg, part, ch) in enumerate(codes):
        base = 23 + 8 * k
        parts += [(base, rj(pos, 5)), (base + 5, neg), (base + 6, part), (base + 7, ch)]
    return place(parts)


def I_and(codes, rel="AND"):
    """An AND or OR line of record identification codes."""
    parts = [(6, "I"), (16, rel)]
    for k, (pos, neg, part, ch) in enumerate(codes):
        base = 23 + 8 * k
        parts += [(base, rj(pos, 5)), (base + 5, neg), (base + 6, part), (base + 7, ch)]
    return place(parts)


def I_fld(name, frm, to, dec="", fmt="", level="", match="", rel="", plus="", minus="", zero=""):
    """A program-described field line."""
    return place([
        (6, "I"), (36, fmt), (37, rj(frm, 5)), (42, rj(to, 5)), (47, rj(dec, 2) if dec != "" else ""),
        (49, name), (63, level), (65, match), (67, rel), (69, plus), (71, minus), (73, zero),
    ])


def I_xrec(fmt, rid=""):
    """An externally described record line."""
    return place([(6, "I"), (7, fmt), (21, rid)])


def I_xfld(ext, name="", level="", match="", plus="", minus="", zero=""):
    """An externally described field line: rename, level, matching, indicators."""
    return place([(6, "I"), (21, ext), (49, name or ext), (63, level), (65, match), (69, plus), (71, minus), (73, zero)])


def A(name="", kind="", length="", typ="", dec="", kw=""):
    """A DDS line."""
    return place([
        (6, "A"), (17, kind), (19, name), (30, rj(length, 5) if length != "" else ""), (35, typ),
        (36, rj(dec, 2) if dec != "" else ""), (45, kw),
    ])
