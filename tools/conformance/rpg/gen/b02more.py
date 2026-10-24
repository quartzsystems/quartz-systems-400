"""Corpus batch 2, members 23 on: more scalars; data structure layouts
with POS, OVERLAY, *NEXT and ALIGN; dates, times and timestamps in every
format and separator; INZ variants; DIM forms; structure forms (EXTNAME,
LIKEREC, LIKEDS, BASED, PSDS, INFDS, data areas, OCCURS); constants and
LIKE. b02.py calls add().
"""
import random
import re

from fixed import D, wrap

FREE_HEAD = ["**FREE", "ctl-opt dftactgrp(*no);", "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"]
FIX_HEAD = [
    "     H DFTACTGRP(*NO)",
    "     FQSYSPRT   O    F  132        PRINTER",
    "     D line            DS           132",
]


def p(expr, ind=""):
    """Print an expression; a long statement goes on over lines."""
    pieces = wrap(f"{ind}line = {expr};", 90)
    return [pieces[0]] + [ind + "  " + x for x in pieces[1:]] + [f"{ind}write QSYSPRT line;"]


def ff(lines):
    """Free-form lines for a fixed-format member, cut to end by column 80."""
    return ["        " + piece for ln in lines for piece in wrap(ln, 72)]


def fixed_member(decls, body):
    return FIX_HEAD + decls + ["      /free"] + ff(body) + ["        *inlr = *on;", "      /end-free"]


def dkw(name="", deftype="", frm="", to="", typ="", dec="", kws=(), ext="", dstype=""):
    """A D spec whose keywords go on continuation lines when they do not fit."""
    out, cur = [], ""
    for k in kws:
        if not k:
            continue
        cand = (cur + " " + k).strip()
        if len(cand) > 37 and cur:
            out.append(cur)
            cur = k
        else:
            cur = cand
    out.append(cur)
    lines = [D(name, deftype, frm, to, typ, dec, out[0], ext, dstype)]
    lines += [D(kw=k) for k in out[1:]]
    return lines


# Date formats a date field may have, and the external formats for %CHAR and %DATE.
DATE_FMTS = ["*ISO", "*USA", "*EUR", "*JIS"] + [f"{f}{s}" for f in ("*MDY", "*DMY", "*YMD", "*JUL") for s in ("/", "-", ".", ",", "&")]
DATE_EXT = ["*ISO0", "*USA0", "*EUR0", "*JIS0", "*MDY0", "*DMY0", "*YMD0", "*JUL0", "*CYMD", "*CMDY", "*CDMY", "*LONGJUL",
            "*CYMD/", "*CMDY-", "*CDMY.", "*LONGJUL,", "*CYMD0", "*LONGJUL0"]
TIME_FMTS = ["*ISO", "*USA", "*EUR", "*JIS"] + [f"*HMS{s}" for s in (":", ".", ",", "&")]
TIME_EXT = ["*ISO0", "*EUR0", "*JIS0", "*HMS0"]


def dsize(fmt):
    base = fmt.rstrip("/-.,&0")
    return {"*ISO": 10, "*USA": 10, "*EUR": 10, "*JIS": 10, "*MDY": 8, "*DMY": 8, "*YMD": 8, "*JUL": 6}[base]


def sub_shape(rng, allow_ptr=True, small=False):
    """A subfield type: free text, fixed (len, type, dec, kw), size, kind."""
    ks = ["char", "char", "varchar", "packed", "zoned", "bindec", "int", "uns", "float", "ind", "date", "time", "timestamp", "ucs2"]
    if allow_ptr:
        ks.append("pointer")
    if small:
        ks = ["char", "packed", "zoned", "int", "uns", "ind"]
    k = rng.choice(ks)
    if k == "char":
        n = rng.randint(1, 12)
        return f"char({n})", (n, "A", "", ""), n, "char"
    if k == "varchar":
        n = rng.randint(1, 10)
        return f"varchar({n})", (n, "A", "", "VARYING"), n + 2, "varchar"
    if k in ("packed", "zoned"):
        dg = rng.randint(1, 15)
        s = rng.randint(0, min(dg, 4))
        sz = dg // 2 + 1 if k == "packed" else dg
        return f"{k}({dg}:{s})", (dg, "P" if k == "packed" else "S", s, ""), sz, "num"
    if k == "bindec":
        dg = rng.randint(1, 9)
        s = rng.randint(0, min(dg, 2))
        return f"bindec({dg}:{s})", (dg, "B", s, ""), 2 if dg <= 4 else 4, "num"
    if k in ("int", "uns"):
        dg = rng.choice([3, 5, 10, 20])
        return f"{k}({dg})", (dg, "I" if k == "int" else "U", 0, ""), {3: 1, 5: 2, 10: 4, 20: 8}[dg], "num"
    if k == "float":
        n = rng.choice([4, 8])
        return f"float({n})", (n, "F", "", ""), n, "float"
    if k == "ind":
        return "ind", ("", "N", "", ""), 1, "ind"
    if k == "date":
        f = rng.choice(DATE_FMTS)
        return f"date({f})", ("", "D", "", f"DATFMT({f})"), dsize(f), "date"
    if k == "time":
        f = rng.choice(TIME_FMTS)
        return f"time({f})", ("", "T", "", f"TIMFMT({f})"), 8, "time"
    if k == "timestamp":
        return "timestamp", ("", "Z", "", ""), 26, "ts"
    if k == "ucs2":
        n = rng.randint(1, 6)
        return f"ucs2({n})", (n, "C", "", ""), 2 * n, "ucs2"
    return "pointer", ("", "*", "", ""), 16, "ptr"


def value_of(ref, kind):
    if kind in ("num", "float"):
        return f"%char({ref})"
    if kind in ("char", "varchar"):
        return f"'[' + {ref} + ']'"
    if kind in ("date", "time", "ts", "ucs2"):
        return f"%char({ref})"
    if kind == "ind":
        return ref
    return "'ptr'"


def ds_member(rng, mode, fixed):
    """A random structure. mode: 'clean' (one after another, values
    printed), 'place' (POS, OVERLAY, *NEXT; character view printed),
    'align' (ALIGN; sizes and offsets)."""
    qual = rng.random() < 0.4 and not fixed
    dsinz = mode == "clean" and rng.random() < 0.5
    subs = []  # (name, free type, fixed, size, kind, placement)
    chars = []  # (name, size, next overlay pos)
    n = rng.randint(4, 9)
    pos_end = 0
    for i in range(n):
        name = f"S{i + 1:02d}"
        place = ""
        if mode == "place" and i > 0 and rng.random() < 0.6:
            big = [c for c in chars if c[1] >= 3]
            if big and rng.random() < 0.6:
                tgt = rng.choice(big)
                free, fx, size, kind = sub_shape(rng, allow_ptr=False, small=True)
                room = tgt[1]
                if size > room:
                    free, fx, size, kind = f"char({room})", (room, "A", "", ""), room, "char"
                how = rng.random()
                if how >= 0.7 and tgt[2] + size - 1 <= room:
                    place = f"overlay({tgt[0]} : *next)"
                    start = tgt[2]
                elif how < 0.3 or how >= 0.7:
                    place = f"overlay({tgt[0]})"
                    start = 1
                else:
                    start = rng.randint(1, room - size + 1)
                    place = f"overlay({tgt[0]} : {start})"
                tgt[2] = start + size
                subs.append((name, free, fx, size, kind, place))
                if kind == "char":
                    chars.append([name, size, 1])
                continue
            if not fixed:
                free, fx, size, kind = sub_shape(rng, allow_ptr=False)
                start = rng.randint(1, pos_end + 6)
                place = f"pos({start})"
                subs.append((name, free, fx, size, kind, place))
                pos_end = max(pos_end, start + size - 1)
                if kind == "char":
                    chars.append([name, size, 1])
                continue
        free, fx, size, kind = sub_shape(rng, allow_ptr=(mode != "place"))
        if mode == "place" and kind == "char" and size < 4:
            free, fx, size = f"char({size + 4})", (size + 4, "A", "", ""), size + 4
        subs.append((name, free, fx, size, kind, place))
        pos_end += size
        if kind == "char":
            chars.append([name, size, 1])
    opts = []
    if qual:
        opts.append("qualified")
    if dsinz:
        opts.append("inz")
    if mode == "align":
        opts.append("align")
    ds = "DS1"
    ref = (lambda s: f"{ds}.{s}") if qual else (lambda s: s)
    # Values for subfields of a clean structure without INZ.
    vals = {}
    if mode == "clean" and not dsinz:
        for name, free, fx, size, kind, _ in subs:
            vals[name] = inz_value(rng, kind, free, fx)
    body = p(f"'size ' + %char(%size({ds}))")
    for name, free, fx, size, kind, _ in subs:
        r = ref(name)
        body += p(f"'{name} ' + %char(%size({r})) + ' ' + %char(%addr({r}) - %addr({ds}))")
        if mode == "clean":
            body += p("'  ' + " + value_of(r, kind))
    if mode == "place":
        text = "".join("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"[(i * 7) % 36] for i in range(60))
        body += [f"{ds} = '{text}';"]
        for name, free, fx, size, kind, _ in subs:
            if kind == "char":
                body += p(f"'{name} [' + {ref(name)} + ']'")
    if fixed:
        decls = dkw(ds, "DS", kws=[o.upper() for o in opts])
        for name, free, fx, size, kind, place in subs:
            ln, typ, dec, extra = fx
            kws = [extra, place.upper().replace(" ", ""), (f"INZ({vals[name]})" if name in vals else "")]
            decls += dkw(" " + name, to=str(ln) if ln != "" else "", typ=typ, dec=str(dec) if dec != "" else "", kws=kws)
        return fixed_member(decls, body)
    lines = FREE_HEAD + [f"dcl-ds {ds}" + "".join(" " + o for o in opts) + ";"]
    for name, free, fx, size, kind, place in subs:
        inz = f" inz({vals[name]})" if name in vals else ""
        lines.append(f"  {name} {free}" + (" " + place if place else "") + inz + ";")
    lines.append("end-ds;")
    return lines + body + ["*inlr = *on;"]


def inz_value(rng, kind, free, fx):
    if kind == "char":
        n = fx[0]
        return "'" + "kqzwmx"[: rng.randint(1, min(n, 6))] + "'"
    if kind == "varchar":
        return "'" + "vw"[: min(fx[0], 2)] + "'"
    if kind == "num":
        dg, s = fx[0], fx[2]
        if fx[1] in ("I", "U"):
            return str(rng.randint(0, 100))
        i = dg - s
        top = 10 ** min(i, 4) - 1 if i else 0
        if fx[1] == "B":
            top = 10 ** min(i, 2) - 1 if i else 0
        v = str(rng.randint(0, top)) if i else "0"
        if s:
            v += "." + "".join(rng.choice("123456789") for _ in range(s))
        return ("-" if rng.random() < 0.3 and fx[1] != "U" else "") + v
    if kind == "float":
        return rng.choice(["1.5", "-0.25", "3E2"])
    if kind == "ind":
        return rng.choice(["*on", "*off"])
    if kind == "date":
        return "d'2026-03-05'"
    if kind == "time":
        return "t'14.30.05'"
    if kind == "ts":
        return "z'2026-03-05-14.30.05.000123'"
    if kind == "ucs2":
        return "u'0075'"
    return "*null"


def dates_member(k, dfmts, tfmts, exts, texts):
    """Date and time fields of the given formats from one ISO value,
    shown as themselves and in external formats."""
    lines = FREE_HEAD + []
    body = []
    for i, f in enumerate(dfmts):
        lines.append(f"dcl-s d{i} date({f}) inz(d'2026-0{(k + i) % 9 + 1}-{(k * 3 + i) % 27 + 1:02d}');")
        body += p(f"'{f} ' + %char(d{i}) + ' ' + %char(%size(d{i}))")
    for i, f in enumerate(tfmts):
        lines.append(f"dcl-s t{i} time({f}) inz(t'{(k + 5 * i) % 24:02d}.{(k * 7 + i) % 60:02d}.0{i}');")
        body += p(f"'{f} ' + %char(t{i}) + ' ' + %char(%size(t{i}))")
    for e in exts:
        body += p(f"'{e} ' + %char(d0 : {e})")
        body += p(f"'  ' + %char(%date(%char(d0 : {e}) : {e}))")
    for e in texts:
        body += p(f"'{e} ' + %char(t0 : {e})")
        body += p(f"'  ' + %char(%time(%char(t0 : {e}) : {e}))")
    return lines + body + ["*inlr = *on;"]


def no_int_digit(ln):
    """INZ(0) of a field whose digits are all decimals is *ZEROS (a
    literal such as 0.5 keeps its zero: pub400 compiled it so)."""
    ln = re.sub(r"(bindec|packed|zoned)\((\d+):\2\) inz\(0\)", r"\1(\2:\2) inz(*zeros)", ln)
    if ln.startswith("     D") and ln[39:40] in "BPS" and ln[32:39].strip() and ln[32:39].strip() == ln[40:42].strip():
        ln = ln.replace("INZ(0)", "INZ(*ZEROS)")
    return ln


def add(M, scalars):
    rng = random.Random(903)
    for n in range(23, 35):
        scalars(f"DCL{n:03d}", rng, fixed=False)
    for n in range(35, 45):
        scalars(f"DCL{n:03d}", rng, fixed=True)
    # A field of decimals only takes no integer digit in its INZ.
    for n in range(23, 45):
        M[f"DCL{n:03d}"] = [no_int_digit(ln) for ln in M[f"DCL{n:03d}"]]

    rng = random.Random(945)
    n = 45
    for mode, count in (("clean", 14), ("place", 18), ("align", 8)):
        for i in range(count):
            M[f"DCL{n:03d}"] = ds_member(rng, mode, fixed=(i % 3 == 2))
            n += 1

    # Dates and times: every field format and every external format.
    dl, tl = list(DATE_FMTS), list(TIME_FMTS)
    for i in range(12):
        dfmts = [dl[(i * 2 + j) % len(dl)] for j in range(3)]
        tfmts = [tl[(i + j) % len(tl)] for j in range(2)]
        exts = DATE_EXT[i * 3 % len(DATE_EXT): i * 3 % len(DATE_EXT) + 3]
        texts = [TIME_EXT[i % len(TIME_EXT)]]
        M[f"DCL{n:03d}"] = dates_member(i, dfmts, tfmts, exts, texts)
        n += 1
    assert n == 97
    hand(M)


def hand(M):
    F = FREE_HEAD

    # Default date and time formats from the control options; literals in them.
    M["DCL097"] = ["**FREE", "ctl-opt dftactgrp(*no) datfmt(*mdy-) timfmt(*usa);"] + F[2:] + [
        "dcl-s d1 date inz(d'03-05-26');",
        "dcl-s d2 date(*iso) inz(d'12-31-39');",
        "dcl-s t1 time inz(t'02:30 PM');",
        "dcl-s t2 time(*iso) inz(t'11:15 AM');",
        "dcl-s ts timestamp inz(z'2026-03-05-14.30.05.123456');",
    ] + p("%char(d1) + ' ' + %char(d2) + ' ' + %char(t1) + ' ' + %char(t2)") + p("%char(%size(d1)) + ' ' + %char(ts)") + ["*inlr = *on;"]

    # Fixed-form dates with DATFMT and TIMFMT, and H-spec defaults.
    M["DCL098"] = [
        "     H DFTACTGRP(*NO) DATFMT(*EUR) TIMFMT(*JIS)",
        "     FQSYSPRT   O    F  132        PRINTER",
        "     D line            DS           132",
        "     D D1              S               D   INZ(D'05.03.2026')",
        "     D D2              S               D   DATFMT(*YMD/) INZ(D'31.12.2039')",
        "     D D3              S               D   DATFMT(*JUL) INZ(D'01.01.1940')",
        "     D T1              S               T   INZ(T'14:30:05')",
        "     D T2              S               T   TIMFMT(*HMS,) INZ(T'00:00:00')",
        "      /free",
    ] + ff(p("%char(D1) + ' ' + %char(D2) + ' ' + %char(D3)") + p("%char(T1) + ' ' + %char(T2) + ' ' + %char(%size(D3))")) + [
        "        *inlr = *on;",
        "      /end-free",
    ]

    # Timestamps with fractional precision.
    M["DCL099"] = F + [
        "dcl-s t0 timestamp(0) inz(z'2026-03-05-14.30.05');",
        "dcl-s t3 timestamp(3) inz(z'2026-03-05-14.30.05.123');",
        "dcl-s t6 timestamp inz(z'2026-03-05-14.30.05.123456');",
        "dcl-s t9 timestamp(9) inz(z'2026-03-05-14.30.05.123456789');",
        "dcl-s t12 timestamp(12) inz(z'2026-03-05-14.30.05.123456789012');",
    ] + p("%char(t0) + ' ' + %char(%size(t0))") + p("%char(t3) + ' ' + %char(%size(t3))") + p("%char(t6) + ' ' + %char(%size(t6))") + p("%char(t9) + ' ' + %char(%size(t9))") + p("%char(t12) + ' ' + %char(%size(t12))") + [
        "t3 = t12;",
        "t12 = t0;",
    ] + p("%char(t3) + ' ' + %char(t12)") + p("%char(t6 : *iso0)") + ["*inlr = *on;"]

    # *SYS and *JOB: only invariants are printed.
    M["DCL100"] = F + [
        "dcl-s d1 date inz(*sys);",
        "dcl-s d2 date(*mdy) inz(*job);",
        "dcl-s t1 time inz(*sys);",
        "dcl-s z1 timestamp inz(*sys);",
        "dcl-s ok ind;",
        "ok = d1 >= d'2026-01-01' and d1 < d'2100-01-01';",
    ] + p("'sys date ok ' + ok") + [
        "ok = %diff(d1 : d2 : *days) >= -1 and %diff(d1 : d2 : *days) <= 1;",
    ] + p("'job date near ' + ok") + [
        "ok = %date(z1) = d1 or %diff(%date(z1) : d1 : *days) = 1;",
    ] + p("'timestamp date ' + ok") + [
        "ok = t1 >= t'00.00.00';",
    ] + p("'time ok ' + ok") + ["*inlr = *on;"]

    # Default initial values of dates in two-digit-year formats.
    M["DCL101"] = F + [
        "dcl-ds ds1 inz;",
        "  d1 date(*mdy);",
        "  d2 date(*iso);",
        "  d3 date(*jul);",
        "  t1 time(*usa);",
        "  z1 timestamp;",
        "end-ds;",
        "dcl-s s1 date(*ymd);",
        "dcl-s s2 time;",
    ] + p("%char(d1) + ' ' + %char(d2) + ' ' + %char(d3) + ' ' + %char(t1)") + p("%char(z1) + ' ' + %char(s1) + ' ' + %char(s2)") + ["*inlr = *on;"]

    # *HIVAL and *LOVAL of every type.
    M["DCL102"] = F + [
        "dcl-s p1 packed(5:2) inz(*hival);",
        "dcl-s p2 packed(5:2) inz(*loval);",
        "dcl-s z1 zoned(4:1) inz(*hival);",
        "dcl-s z2 zoned(4:1) inz(*loval);",
        "dcl-s b1 bindec(4:0) inz(*hival);",
        "dcl-s b2 bindec(9:2) inz(*loval);",
        "dcl-s i1 int(5) inz(*hival);",
        "dcl-s i2 int(10) inz(*loval);",
        "dcl-s i3 int(20) inz(*hival);",
        "dcl-s i4 int(3) inz(*loval);",
        "dcl-s u1 uns(5) inz(*hival);",
        "dcl-s u2 uns(10) inz(*loval);",
        "dcl-s f1 float(8) inz(*hival);",
        "dcl-s f2 float(4) inz(*loval);",
    ] + p("%char(p1) + ' ' + %char(p2) + ' ' + %char(z1) + ' ' + %char(z2)") + p("%char(b1) + ' ' + %char(b2) + ' ' + %char(i1) + ' ' + %char(i2)") + p("%char(i3) + ' ' + %char(i4) + ' ' + %char(u1) + ' ' + %char(u2)") + p("%char(f1) + ' ' + %char(f2)") + ["*inlr = *on;"]

    M["DCL103"] = F + [
        "dcl-s c1 char(3) inz(*hival);",
        "dcl-s c2 char(3) inz(*loval);",
        "dcl-s v1 varchar(4) inz(*hival);",
        "dcl-s v2 varchar(4) inz(*loval);",
        "dcl-s d1 date inz(*hival);",
        "dcl-s d2 date(*mdy) inz(*loval);",
        "dcl-s d3 date(*mdy) inz(*hival);",
        "dcl-s t1 time inz(*hival);",
        "dcl-s t2 time inz(*loval);",
        "dcl-s z1 timestamp inz(*hival);",
        "dcl-s z2 timestamp inz(*loval);",
        "dcl-s u1 ucs2(2) inz(*hival);",
        "dcl-s k1 ind;",
        "dcl-s k2 ind;",
        "dcl-s k3 ind;",
        "dcl-s k4 ind;",
        "k1 = c1 = x'FFFFFF';",
        "k2 = c2 = x'000000';",
        "k3 = v1 = x'FFFFFFFF';",
        "k4 = u1 = u'FFFFFFFF';",
    ] + p("k1 + k2 + ' ' + %char(%len(v1)) + k3 + %char(%len(v2)) + k4") + p("%char(d1) + ' ' + %char(d2) + ' ' + %char(d3)") + p("%char(t1) + ' ' + %char(t2) + ' ' + %char(z1) + ' ' + %char(z2)") + ["*inlr = *on;"]

    # *BLANKS and *ZEROS.
    M["DCL104"] = F + [
        "dcl-s c1 char(4) inz(*blanks);",
        "dcl-s c2 char(4) inz(*zeros);",
        "dcl-s v1 varchar(4) inz(*blanks);",
        "dcl-s v2 varchar(4) inz(*zeros);",
        "dcl-s p1 packed(5:2) inz(*zeros);",
        "dcl-s z1 zoned(3:0) inz(*zeros);",
        "dcl-s i1 int(10) inz(*zeros);",
        "dcl-s f1 float(8) inz(*zeros);",
        "dcl-s u1 ucs2(3) inz(*blanks);",
        "dcl-ds ds1 inz;",
        "  a char(2);",
        "  b packed(3:0);",
        "end-ds;",
    ] + p("'[' + c1 + '][' + c2 + '][' + v1 + '][' + v2 + '] ' + %char(%len(v1)) + %char(%len(v2))") + p("%char(p1) + ' ' + %char(z1) + ' ' + %char(i1) + ' ' + %char(f1)") + p("'[' + %char(u1) + '] ' + a + ' ' + %char(b)") + ["*inlr = *on;"]

    # *USER: printed only as a property.
    M["DCL105"] = F + [
        "dcl-s u1 char(10) inz(*user);",
        "dcl-s u2 varchar(18) inz(*user);",
        "dcl-s k1 ind;",
        "dcl-s k2 ind;",
        "k1 = %len(%trim(u1)) > 0;",
        "k2 = %trim(u1) = u2;",
    ] + p("k1 + ' ' + k2") + ["*inlr = *on;"]

    # INZ(*EXTDFT) from the file's DFT values.
    M["DCL106"] = ["**FREE", "//; dds: DFTF"] + F[1:] + [
        "dcl-ds d1 extname('DFTF') inz(*extdft) end-ds;",
        "dcl-ds d2 extname('DFTF') qualified inz end-ds;",
        "dcl-ds d3 extname('DFTF') qualified inz(*extdft);",
        "  dqty extfld inz(9);",
        "end-ds;",
    ] + p("dcode + ' ' + %char(dqty) + ' ' + %char(damt) + ' [' + dname + '] ' + %char(dcnt)") + p("%char(ddate) + ' [' + dtext + '] ' + %char(%len(dtext))") + p("'[' + d2.dcode + '] ' + %char(d2.dqty) + ' ' + %char(d2.ddate) + ' ' + %char(%len(d2.dtext))") + p("d3.dcode + ' ' + %char(d3.dqty)") + ["*inlr = *on;"]

    # Numeric literal forms.
    M["DCL107"] = F + [
        "dcl-s a packed(7:3) inz(-.5);",
        "dcl-s b packed(7:3) inz(+12.25);",
        "dcl-s c zoned(5:0) inz(-00042);",
        "dcl-s d float(8) inz(1.25E3);",
        "dcl-s e float(8) inz(-2e-2);",
        "dcl-s f float(4) inz(7);",
        "dcl-s g int(10) inz(-2147483648);",
        "dcl-s h uns(20) inz(18446744073709551615);",
        "dcl-s i packed(31:0) inz(9999999999999999999999999999999);",
        "dcl-s j packed(9:9) inz(.123456789);",
    ] + p("%char(a) + ' ' + %char(b) + ' ' + %char(c)") + p("%char(d) + ' ' + %char(e) + ' ' + %char(f)") + p("%char(g) + ' ' + %char(h)") + p("%char(i) + ' ' + %char(j)") + ["*inlr = *on;"]

    # Character literal forms: quotes, hex, *ALL, UCS-2.
    M["DCL108"] = F + [
        "dcl-s a char(8) inz('it''s');",
        "dcl-s b char(4) inz(x'C1C2C3C4');",
        "dcl-s c char(7) inz(*all'ab');",
        "dcl-s d char(5) inz(*allx'F1');",
        "dcl-s e varchar(10) inz('');",
        "dcl-s f ucs2(4) inz(u'00410042');",
        "dcl-s g varchar(6) inz(*all'xy');",
        "dcl-s h char(3) inz('');",
        "dcl-s i ucs2(3) inz(u'006100620063');",
    ] + p("'[' + a + '][' + b + '][' + c + '][' + d + '][' + e + '] ' + %char(%len(e))") + p("'[' + %char(f) + '][' + g + '] ' + %char(%len(g)) + ' [' + h + '] ' + %char(i)") + ["*inlr = *on;"]

    # Built-ins and constants as initial values.
    M["DCL109"] = F + [
        "dcl-c K 25;",
        "dcl-c TXT 'const';",
        "dcl-s arr char(6) dim(4);",
        "dcl-s a int(10) inz(%size(arr));",
        "dcl-s b int(10) inz(%elem(arr));",
        "dcl-s c int(10) inz(%len(TXT));",
        "dcl-s d packed(5:1) inz(K);",
        "dcl-s e char(8) inz(TXT);",
        "dcl-s f int(10) inz(%size(arr : *all));",
        "dcl-s g int(10) inz(%decpos(d));",
    ] + p("%char(a) + ' ' + %char(b) + ' ' + %char(c) + ' ' + %char(d) + ' ' + e + ' ' + %char(f) + ' ' + %char(g)") + ["*inlr = *on;"]

    # Pointer and procedure pointer initial values.
    M["DCL110"] = F + [
        "dcl-s buf char(10) inz('0123456789');",
        "dcl-s p1 pointer inz(%addr(buf));",
        "dcl-s p2 pointer inz(*null);",
        "dcl-pr twice int(10);",
        "  n int(10) value;",
        "end-pr;",
        "dcl-s pp pointer(*proc) inz(%paddr(twice));",
        "dcl-s over char(3) based(p1);",
        "dcl-pr call int(10) extproc(pp);",
        "  n int(10) value;",
        "end-pr;",
        "dcl-s k1 ind;",
        "k1 = p2 = *null;",
    ] + p("over + ' ' + k1 + ' ' + %char(call(21))") + [
        "*inlr = *on;",
        "dcl-proc twice;",
        "  dcl-pi *n int(10);",
        "    n int(10) value;",
        "  end-pi;",
        "  return n * 2;",
        "end-proc;",
    ]

    # Structure INZ against subfield INZ.
    M["DCL111"] = F + [
        "dcl-ds a inz;",
        "  a1 char(3);",
        "  a2 packed(5:2) inz(1.5);",
        "  a3 zoned(3:0);",
        "  a4 int(5);",
        "  a5 ind;",
        "  a6 varchar(5);",
        "  a7 date;",
        "end-ds;",
        "dcl-ds b;",
        "  b1 char(3) inz('bb');",
        "  b2 zoned(3:0) inz;",
        "  b3 char(2);",
        "end-ds;",
    ] + p("'[' + a1 + '] ' + %char(a2) + ' ' + %char(a3) + ' ' + %char(a4) + ' ' + a5 + ' ' + %char(%len(a6)) + ' ' + %char(a7)") + p("'[' + b + '] ' + %char(b2)") + ["*inlr = *on;"]

    # INZ in fixed form, keywords continued, literal continued.
    M["DCL112"] = [
        "     H DFTACTGRP(*NO)",
        "     FQSYSPRT   O    F  132        PRINTER",
        "     D line            DS           132",
        "     D A               S             40    VARYING",
        "     D                                     INZ('a value continued +",
        "     D                                     here')",
        "     D B               S              7P 2 INZ(*HIVAL)",
        "     D C               S              3    INZ(*ALL'-')",
        "     D E               S               D   INZ(*LOVAL) DATFMT(*USA)",
        "     D F               S              5I 0 INZ(-32768)",
        "     D G               S              4    INZ(X'F1F2F3F4')",
        "      /free",
    ] + ff(p("'[' + A + '] ' + %char(B) + ' ' + C + ' ' + %char(E) + ' ' + %char(F) + ' ' + G")) + [
        "        *inlr = *on;",
        "      /end-free",
    ]

    # Initial values of array elements and RESET to them.
    M["DCL113"] = F + [
        "dcl-s a packed(3:0) dim(3) inz(7);",
        "dcl-s b char(2) dim(2) inz(*all'z');",
        "dcl-ds s dim(2) qualified inz;",
        "  x int(5) inz(4);",
        "  y char(2);",
        "end-ds;",
        "a(2) = 1;",
        "s(2).x = 9;",
    ] + p("%char(a(1)) + %char(a(2)) + %char(a(3)) + ' ' + b(1) + b(2) + ' ' + %char(s(1).x) + %char(s(2).x) + '[' + s(1).y + ']'") + [
        "reset a;",
        "reset s;",
    ] + p("%char(a(1)) + %char(a(2)) + %char(a(3)) + ' ' + %char(s(1).x) + %char(s(2).x)") + ["*inlr = *on;"]

    # Graphic fields: sizes and lengths.
    M["DCL114"] = F + [
        "dcl-s g1 graph(5);",
        "dcl-s g3 varucs2(5 : 4);",
        "dcl-s u1 ucs2(5);",
        "dcl-s u2 varucs2(5) inz(u'00610062');",
        "dcl-s c1 varchar(65535);",
        "dcl-s c2 varchar(10 : 2);",
    ] + p("%char(%size(g1)) + ' ' + %char(%size(g3)) + ' ' + %char(%size(u1))") + p("%char(%size(u2)) + ' ' + %char(%len(u2)) + ' ' + %char(%size(c1)) + ' ' + %char(%size(c2))") + ["*inlr = *on;"]

    # Fixed-form graphic, UCS-2 and varying sizes.
    M["DCL115"] = [
        "     H DFTACTGRP(*NO)",
        "     FQSYSPRT   O    F  132        PRINTER",
        "     D line            DS           132",
        "     D G1              S              5G",
        "     D U1              S              5C",
        "     D U2              S              5C   VARYING(4)",
        "     D V1              S          65535A   VARYING",
        "     D V2              S             10A   VARYING(2)",
        "      /free",
    ] + ff(p("%char(%size(G1)) + ' ' + %char(%size(U1)) + ' ' + %char(%size(U2)) + ' ' + %char(%size(V1)) + ' ' + %char(%size(V2))")) + [
        "        *inlr = *on;",
        "      /end-free",
    ]

    # Arrays: DIM from constants and built-ins, %ELEM, %SIZE.
    M["DCL116"] = F + [
        "dcl-c N 3;",
        "dcl-s a char(4) dim(N);",
        "dcl-s b packed(7:2) dim(%elem(a));",
        "dcl-s c int(10) dim(10);",
        "dcl-s d like(b) dim(2);",
        "dcl-s e varchar(5) dim(4);",
        "dcl-s f date dim(2);",
        "dcl-s g ind dim(99);",
    ] + p("%char(%elem(a)) + ' ' + %char(%size(a)) + ' ' + %char(%size(a : *all))") + p("%char(%elem(b)) + ' ' + %char(%size(b : *all)) + ' ' + %char(%size(c : *all))") + p("%char(%elem(d)) + ' ' + %char(%size(d)) + ' ' + %char(%size(e)) + ' ' + %char(%size(e : *all))") + p("%char(%size(f : *all)) + ' ' + %char(%elem(g))") + ["*inlr = *on;"]

    # DIM(*AUTO) and DIM(*VAR).
    M["DCL117"] = F + [
        "dcl-s a int(10) dim(*auto : 10);",
        "dcl-s v char(3) dim(*var : 5);",
        "dcl-s i int(10);",
    ] + p("%char(%elem(a)) + ' ' + %char(%elem(a : *max)) + ' ' + %char(%elem(v))") + [
        "a(*next) = 5;",
        "a(*next) = 7;",
        "a(4) = 9;",
        "%elem(v) = 2;",
        "v(1) = 'aa';",
        "v(2) = 'bb';",
    ] + p("%char(%elem(a)) + ' ' + %char(a(1)) + %char(a(2)) + %char(a(3)) + %char(a(4)) + ' ' + %char(%elem(v)) + v(1) + v(2)") + [
        "%elem(a) = 1;",
        "%elem(v) = 4;",
    ] + p("%char(%elem(a)) + ' ' + %char(%elem(v)) + '[' + v(4) + ']'") + ["*inlr = *on;"]

    # Compile-time arrays with PERRCD, default PERRCD and dates.
    M["DCL118"] = F + [
        "dcl-s a char(4) dim(3) ctdata;",
        "dcl-s b zoned(3:1) dim(6) ctdata perrcd(3);",
        "dcl-s c date(*iso) dim(2) ctdata;",
        "dcl-s d char(2) dim(5) ctdata perrcd(2);",
    ] + p("a(1) + a(2) + a(3)") + p("%char(b(1)) + ' ' + %char(b(3)) + ' ' + %char(b(6))") + p("%char(c(1)) + ' ' + %char(c(2))") + p("d(1) + d(2) + d(3) + d(4) + '[' + d(5) + ']'") + [
        "*inlr = *on;",
        "**CTDATA a",
        "one",
        "two",
        "thre",
        "**CTDATA b",
        "010025999",
        "100200300",
        "**CTDATA c",
        "2026-01-31",
        "1999-12-31",
        "**CTDATA d",
        "aabb",
        "ccdd",
        "ee",
    ]

    # Alternating arrays in free form and their order.
    M["DCL119"] = F + [
        "dcl-s code char(1) dim(4) ctdata perrcd(2) ascend;",
        "dcl-s amt packed(5:2) dim(4) alt(code);",
        "dcl-s i int(10);",
        "for i = 1 to 4;",
    ] + p("code(i) + ' ' + %char(amt(i))", "  ") + [
        "endfor;",
    ] + p("%char(%lookup('C' : code)) + ' ' + %char(%lookupgt('B' : code)) + ' ' + %char(%lookuplt('B' : code))") + [
        "*inlr = *on;",
        "**CTDATA code",
        "A00125B10000",
        "C00001D99999",
    ]

    # ASCEND and DESCEND with SORTA.
    M["DCL120"] = F + [
        "dcl-s a int(10) dim(5) ascend;",
        "dcl-s d char(2) dim(4) descend;",
        "a(1) = 9;",
        "a(2) = 3;",
        "a(3) = 7;",
        "a(4) = 1;",
        "a(5) = 5;",
        "d(1) = 'bb';",
        "d(2) = 'dd';",
        "d(3) = 'aa';",
        "d(4) = 'cc';",
        "sorta a;",
        "sorta d;",
    ] + p("%char(a(1)) + %char(a(2)) + %char(a(3)) + %char(a(4)) + %char(a(5)) + ' ' + d(1) + d(2) + d(3) + d(4)") + p("%char(%lookupge(4 : a)) + ' ' + %char(%lookuple('bc' : d))") + ["*inlr = *on;"]

    # Arrays of structures, nested.
    M["DCL121"] = F + [
        "dcl-ds item qualified template;",
        "  code char(2);",
        "  qty int(5) dim(3);",
        "end-ds;",
        "dcl-ds order qualified;",
        "  id int(10);",
        "  items likeds(item) dim(2);",
        "end-ds;",
        "dcl-ds orders likeds(order) dim(3);",
        "orders(2).items(2).qty(3) = 77;",
        "orders(3).items(1).code = 'zz';",
    ] + p("%char(%size(item)) + ' ' + %char(%size(order)) + ' ' + %char(%size(orders)) + ' ' + %char(%size(orders : *all))") + p("%char(%elem(orders)) + ' ' + %char(%elem(order.items)) + ' ' + %char(%elem(order.items(1).qty))") + p("%char(orders(2).items(2).qty(3)) + ' ' + orders(3).items(1).code") + p("%char(%addr(orders(2)) - %addr(orders(1))) + ' ' + %char(%addr(orders(1).items(2)) - %addr(orders(1)))") + ["*inlr = *on;"]

    # Subfield arrays overlaid on each other.
    M["DCL122"] = F + [
        "dcl-ds tbl qualified dim(3);",
        "  row char(12);",
        "  key char(2) overlay(row);",
        "  val char(5) overlay(row : *next);",
        "  rest char(5) overlay(row : *next);",
        "end-ds;",
        "tbl(1).row = 'k2value2rst2';",
        "tbl(2).row = 'k3value3rst3';",
        "tbl(3).row = 'k1value1rst1';",
    ] + p("tbl(1).key + tbl(2).key + tbl(3).key + ' ' + tbl(2).val + ' ' + tbl(3).rest") + p("%char(%elem(tbl)) + ' ' + %char(%size(tbl.key)) + ' ' + %char(%size(tbl : *all))") + [
        "sorta tbl(*).key;",
    ] + p("tbl(1).row + ' ' + tbl(3).row") + [
        "sorta(d) tbl(*).val;",
    ] + p("tbl(1).row + ' ' + tbl(3).row") + ["*inlr = *on;"]

    # Fixed-form arrays, tables and ALT.
    M["DCL123"] = [
        "     H DFTACTGRP(*NO)",
        "     FQSYSPRT   O    F  132        PRINTER",
        "     D line            DS           132",
        "     D ARR             S              3  0 DIM(4) CTDATA PERRCD(4)",
        "     D ARA             S              2    DIM(3) CTDATA PERRCD(1)",
        "     D ARB             S              4    DIM(3) ALT(ARA)",
        "     D ARR2            S                   LIKE(ARR) DIM(2)",
        "      /free",
    ] + ff(p("%char(ARR(1)) + ' ' + %char(ARR(4)) + ' ' + ARA(2) + ARB(2) + ' ' + %char(%elem(ARR2)) + ' ' + %char(%size(ARR2))")) + [
        "        *inlr = *on;",
        "      /end-free",
        "**CTDATA ARR",
        "001002003004",
        "**CTDATA ARA",
        "X1one",
        "X2two",
        "X3thr",
    ]

    # Arrays of every type with initial values.
    M["DCL124"] = F + [
        "dcl-s a1 zoned(3:1) dim(2) inz(1.5);",
        "dcl-s a2 bindec(4:0) dim(2) inz(-9);",
        "dcl-s a3 uns(3) dim(2) inz(255);",
        "dcl-s a4 float(4) dim(2) inz(0.5);",
        "dcl-s a5 ind dim(2) inz(*on);",
        "dcl-s a6 date(*dmy) dim(2) inz(d'2026-12-01');",
        "dcl-s a7 time(*hms) dim(2) inz(t'23.00.01');",
        "dcl-s a8 timestamp dim(2) inz(z'2026-12-01-23.00.01.000000');",
        "dcl-s a9 pointer dim(2);",
    ] + p("%char(a1(2)) + ' ' + %char(a2(2)) + ' ' + %char(a3(2)) + ' ' + %char(a4(2)) + ' ' + a5(2)") + p("%char(a6(2)) + ' ' + %char(a7(2)) + ' ' + %char(a8(2)) + ' ' + %char(%size(a9 : *all))") + ["*inlr = *on;"]

    # EXTNAME with *ALL, *INPUT, *OUTPUT and *KEY.
    M["DCL125"] = ["**FREE", "//; dds: CUSTF"] + F[1:] + [
        "dcl-ds a extname('CUSTF' : *all) qualified end-ds;",
        "dcl-ds i extname('CUSTF' : *input) qualified end-ds;",
        "dcl-ds o extname('CUSTF' : *output) qualified end-ds;",
        "dcl-ds k extname('CUSTF' : *key) qualified end-ds;",
        "k.cusno = 101;",
        "a.name = 'all';",
    ] + p("%char(%size(a)) + ' ' + %char(%size(i)) + ' ' + %char(%size(o)) + ' ' + %char(%size(k))") + p("%char(k.cusno) + ' ' + %trim(a.name) + ' ' + %char(%size(k.cusno))") + ["*inlr = *on;"]

    # EXTNAME of a two-key file; PREFIX and EXTFLD.
    M["DCL126"] = ["**FREE", "//; dds: ORDF"] + F[1:] + [
        "dcl-ds k extname('ORDF' : *key) qualified end-ds;",
        "dcl-ds r extname('ORDF') prefix(o_) end-ds;",
        "dcl-ds xd extname('ORDF') prefix('XX' : 2) end-ds;",
        "dcl-ds e extname('ORDF') qualified;",
        "  myitem extfld('ITEM');",
        "  price extfld inz(1.5);",
        "end-ds;",
        "o_item = 'widget';",
        "xxdcus = 5;",
        "e.myitem = 'renamed';",
    ] + p("%char(%size(k)) + ' ' + %char(%size(r)) + ' ' + o_item + ' ' + %char(xxdcus)") + p("e.myitem + ' ' + %char(e.price) + ' ' + %char(%size(e.myitem))") + ["*inlr = *on;"]

    # Fixed-form external structures.
    M["DCL127"] = [
        "     H DFTACTGRP(*NO)",
        "      *; dds: TYPESF",
        "     FQSYSPRT   O    F  132        PRINTER",
        "     D line            DS           132",
        "     D TY            E DS                  EXTNAME(TYPESF)",
        "     D TK            E DS                  EXTNAME(TYPESF:*KEY)",
        "     D                                     QUALIFIED",
        "     D TQ            E DS                  EXTNAME(TYPESF) QUALIFIED",
        "     D                                     PREFIX(Q_)",
        "      /free",
    ] + ff(p("%char(%size(TY)) + ' ' + %char(%size(TK)) + ' ' + %char(%size(TQ))") + p("%char(%size(TDATE)) + ' ' + %char(%size(TTIME)) + ' ' + %char(%size(TSTMP)) + ' ' + %char(%size(TVAR))") + p("%char(%size(TBIN)) + ' ' + %char(%size(TBIN2)) + ' ' + %char(%size(TQ.Q_TZON))")) + [
        "        *inlr = *on;",
        "      /end-free",
    ]

    # LIKEREC with *ALL, *KEY, *INPUT and *OUTPUT.
    M["DCL128"] = ["**FREE", "//; dds: ORDF", "//; sqlfile: ORDF"] + F[1:2] + [
        "dcl-f ordf keyed;",
    ] + F[2:] + [
        "dcl-ds a likerec(ordr);",
        "dcl-ds k likerec(ordr : *key);",
        "dcl-ds i likerec(ordr : *input);",
        "dcl-ds o likerec(ordr : *output);",
        "read ordr i;",
        "k.ordcus = i.ordcus;",
        "k.ordno = i.ordno;",
        "chain %kds(k) ordr a;",
    ] + p("%char(%size(a)) + ' ' + %char(%size(k)) + ' ' + %char(%size(i)) + ' ' + %char(%size(o))") + p("%char(a.ordcus) + ' ' + %char(a.ordno) + ' ' + %trim(a.item)") + ["*inlr = *on;"]

    # LIKEDS three levels deep; INZ(*LIKEDS).
    M["DCL129"] = F + [
        "dcl-ds lvl3 qualified template;",
        "  z char(2) inz('zz');",
        "end-ds;",
        "dcl-ds lvl2 qualified template;",
        "  y int(5) inz(2);",
        "  in3 likeds(lvl3) inz(*likeds);",
        "end-ds;",
        "dcl-ds lvl1 qualified inz;",
        "  x char(1) inz('x');",
        "  in2 likeds(lvl2) inz(*likeds);",
        "end-ds;",
        "dcl-ds cpy likeds(lvl1) inz(*likeds);",
        "dcl-ds bare likeds(lvl2);",
    ] + p("lvl1.x + ' ' + %char(lvl1.in2.y) + ' ' + lvl1.in2.in3.z + ' ' + %char(%size(lvl1))") + p("cpy.x + ' ' + %char(cpy.in2.y) + ' ' + cpy.in2.in3.z + ' [' + bare.in3.z + ']'") + ["*inlr = *on;"]

    # Templates: no storage, INZ for LIKEDS copies.
    M["DCL130"] = F + [
        "dcl-s tc char(5) template inz('tchr');",
        "dcl-s c2 like(tc);",
        "dcl-ds td template;",
        "  f1 packed(5:0) inz(5);",
        "end-ds;",
        "dcl-ds d1 likeds(td) inz(*likeds);",
    ] + p("'[' + c2 + '] ' + %char(%size(tc)) + ' ' + %char(d1.f1) + ' ' + %char(%size(td))") + ["*inlr = *on;"]

    # BASED structures and fields over a buffer.
    M["DCL131"] = F + [
        "dcl-s buf char(30) inz('aaaabbbbccccddddeeeeffffgggghh');",
        "dcl-s p pointer;",
        "dcl-ds rec based(p) qualified;",
        "  a char(4);",
        "  n zoned(4:0);",
        "end-ds;",
        "dcl-ds arr based(p) qualified dim(3);",
        "  piece char(4);",
        "end-ds;",
        "dcl-s one char(1) based(p);",
        "p = %addr(buf);",
    ] + p("rec.a + ' ' + arr(2).piece + ' ' + arr(3).piece + ' ' + one") + [
        "p += 8;",
        "rec.a = 'XXXX';",
    ] + p("buf") + [
        "p = %addr(buf);",
    ] + p("one + ' ' + %char(%size(rec)) + ' ' + %char(%size(arr : *all))") + ["*inlr = *on;"]

    # Program status data structure keywords.
    M["DCL132"] = F + [
        "dcl-ds pgmsts psds;",
        "  proc *proc;",
        "  stat *status;",
        "  rtn *routine;",
        "  parms *parms;",
        "end-ds;",
        "dcl-s z int(10) inz(0);",
        "dcl-s r int(10);",
    ] + p("%trim(proc) + ' ' + %char(stat) + ' ' + %trim(rtn) + ' ' + %char(parms) + ' ' + %char(%size(pgmsts))") + [
        "monitor;",
        "  r = 10 / z;",
        "on-error;",
    ] + p("'after error ' + %char(stat) + ' ' + %char(%status)", "  ") + [
        "endmon;",
        "*inlr = *on;",
    ]

    # The program status data structure in fixed form, by positions.
    M["DCL133"] = [
        "     H DFTACTGRP(*NO)",
        "     FQSYSPRT   O    F  132        PRINTER",
        "     D line            DS           132",
        "     D PSDS           SDS",
        "     D  PGMNAM                 1     10",
        "     D  STS                   11     15  0",
        "     D  PRVSTS                16     20  0",
        "     D  RTN                   29     36",
        "     D  NPARMS                37     39  0",
        "     D  MSGID                 40     46",
        "      /free",
    ] + ff(p("%trim(PGMNAM) + ' ' + %char(STS) + ' ' + %char(PRVSTS) + ' ' + %trim(RTN) + ' ' + %char(NPARMS)") + p("'[' + MSGID + '] ' + %char(%size(PSDS))")) + [
        "        *inlr = *on;",
        "      /end-free",
    ]

    # File information data structure.
    M["DCL134"] = ["**FREE", "//; dds: CUSTF", "//; sqlfile: CUSTF"] + F[1:2] + [
        "dcl-f custf keyed infds(fb);",
    ] + F[2:] + [
        "dcl-ds fb;",
        "  fname *file;",
        "  fopen ind pos(9);",
        "  feof ind pos(10);",
        "  fstat *status;",
        "  fop *opcode;",
        "  frec *record;",
        "  ofname char(10) pos(83);",
        "  ofrlen int(5) pos(125);",
        "  ofmbr char(10) pos(129);",
        "end-ds;",
        "chain (102) custr;",
    ] + p("fname + ' ' + fopen + ' ' + %char(fstat) + ' [' + fop + '] ' + %trim(frec)") + p("%trim(ofname) + ' ' + %char(ofrlen) + ' ' + %trim(ofmbr) + ' ' + %char(%size(fb))") + [
        "chain (999) custr;",
    ] + p("%char(fstat) + ' [' + fop + ']'") + ["*inlr = *on;"]

    # Multiple-occurrence structures and %OCCUR.
    M["DCL135"] = F + [
        "dcl-ds mo occurs(3);",
        "  code char(2);",
        "  qty zoned(3:0);",
        "end-ds;",
        "dcl-s i int(10);",
        "for i = 1 to 3;",
        "  %occur(mo) = i;",
        "  code = 'c' + %char(i);",
        "  qty = i * 11;",
        "endfor;",
        "%occur(mo) = 2;",
    ] + p("code + ' ' + %char(qty) + ' ' + %char(%occur(mo)) + ' ' + %char(%size(mo)) + ' ' + %char(%size(mo : *all)) + ' ' + %char(%elem(mo))") + [
        "%occur(mo) = 3;",
    ] + p("mo") + ["*inlr = *on;"]

    # OCCURS in fixed form with the OCCUR operation.
    M["DCL136"] = [
        "     H DFTACTGRP(*NO)",
        "     FQSYSPRT   O    F  132        PRINTER",
        "     D line            DS           132",
        "     D MO              DS                  OCCURS(4) INZ",
        "     D  A                             3",
        "     D  B                             3  0",
        "     D N               S              3  0",
        "     C     2             OCCUR     MO",
        "     C                   EVAL      A = 'two'",
        "     C                   EVAL      B = 22",
        "     C     4             OCCUR     MO",
        "     C                   EVAL      A = 'for'",
        "     C                   OCCUR     MO            N",
        "     C     2             OCCUR     MO",
        "      /free",
    ] + ff(p("A + ' ' + %char(B) + ' ' + %char(N) + ' ' + %char(%size(MO : *ALL))")) + [
        "        %occur(MO) = 1;",
    ] + ff(p("'[' + A + '] ' + %char(B)")) + [
        "        *inlr = *on;",
        "      /end-free",
    ]

    # ALIGN of integer, float and pointer subfields.
    M["DCL137"] = F + [
        "dcl-ds al align qualified;",
        "  c1 char(1);",
        "  i2 int(5);",
        "  c3 char(1);",
        "  i4 int(10);",
        "  c5 char(3);",
        "  i8 int(20);",
        "  f4 float(4);",
        "  c6 char(1);",
        "  f8 float(8);",
        "  u2 uns(5);",
        "  b4 bindec(9);",
        "  pt pointer;",
        "end-ds;",
        "dcl-ds na qualified;",
        "  c1 char(1);",
        "  i4 int(10);",
        "  pt pointer;",
        "end-ds;",
    ] + p("%char(%size(al)) + ' ' + %char(%addr(al.i2) - %addr(al)) + ' ' + %char(%addr(al.i4) - %addr(al)) + ' ' + %char(%addr(al.i8) - %addr(al))") + p("%char(%addr(al.f4) - %addr(al)) + ' ' + %char(%addr(al.f8) - %addr(al)) + ' ' + %char(%addr(al.u2) - %addr(al)) + ' ' + %char(%addr(al.b4) - %addr(al))") + p("%char(%addr(al.pt) - %addr(al)) + ' ' + %char(%size(na)) + ' ' + %char(%addr(na.pt) - %addr(na))") + ["*inlr = *on;"]

    # A data-area structure over the local data area.
    M["DCL138"] = [
        "     H DFTACTGRP(*NO)",
        "     FQSYSPRT   O    F  132        PRINTER",
        "     D line            DS           132",
        "     D LDA            UDS                  DTAARA(*LDA)",
        "     D  LFLD                   1     10",
        "     D  LNUM                  11     15  0",
        "      /free",
        "        LFLD = 'lda field';",
        "        LNUM = 12345;",
    ] + ff(p("LFLD + ' ' + %char(LNUM) + ' ' + %char(%size(LDA))")) + [
        "        *inlr = *on;",
        "      /end-free",
    ]

    # Data areas in free form: DTAARA(*AUTO), IN and OUT.
    M["DCL139"] = F + [
        "dcl-ds lda dtaara(*auto : *usrctl : *lda) len(1024);",
        "  first char(8) pos(101);",
        "end-ds;",
        "first = 'auto-lda';",
        "out lda;",
    ] + p("first + ' ' + %char(%size(lda))") + ["*inlr = *on;"]

    # A structure LEN longer than its subfields; subfields past it.
    M["DCL140"] = F + [
        "dcl-ds a len(20);",
        "  a1 char(3);",
        "end-ds;",
        "dcl-ds b;",
        "  b1 char(3) pos(10);",
        "  b2 char(2);",
        "end-ds;",
        "dcl-ds c len(5) end-ds;",
        "c = 'abcdefg';",
    ] + p("%char(%size(a)) + ' ' + %char(%size(b)) + ' ' + %char(%addr(b2) - %addr(b)) + ' ' + c") + ["*inlr = *on;"]

    # Subfields after POS and an overlaid structure: where *NEXT goes.
    M["DCL141"] = F + [
        "dcl-ds a;",
        "  x char(4) pos(9);",
        "  y char(2);",
        "  z char(3) pos(2);",
        "  w char(1);",
        "end-ds;",
    ] + p("%char(%size(a)) + ' ' + %char(%addr(y) - %addr(a)) + ' ' + %char(%addr(w) - %addr(a))") + ["*inlr = *on;"]

    # Qualified names that repeat global names.
    M["DCL142"] = F + [
        "dcl-s code char(4) inz('glob');",
        "dcl-ds one qualified;",
        "  code char(4) inz('one');",
        "end-ds;",
        "dcl-ds two qualified;",
        "  code char(4) inz('two');",
        "  one char(2) inz('11');",
        "end-ds;",
    ] + p("code + ' ' + one.code + ' ' + two.code + ' ' + two.one") + ["*inlr = *on;"]

    # LIKE with length adjustments.
    M["DCL143"] = F + [
        "dcl-s c char(5) inz('abc');",
        "dcl-s v varchar(5) inz('vv');",
        "dcl-s pk packed(7:2) inz(1.5);",
        "dcl-s zn zoned(6:1) inz(2.5);",
        "dcl-s bn bindec(4:0) inz(7);",
        "dcl-s c2 like(c : +3);",
        "dcl-s c3 like(c : -2);",
        "dcl-s v2 like(v : +5);",
        "dcl-s p2 like(pk : +4);",
        "dcl-s p3 like(pk : -3);",
        "dcl-s z2 like(zn : +1);",
        "dcl-s b2 like(bn : +5);",
        "dcl-s u2 ucs2(3);",
        "dcl-s u3 like(u2 : +2);",
    ] + p("%char(%len(c2)) + ' ' + %char(%len(c3)) + ' ' + %char(%size(v2)) + ' ' + %char(%len(p2)) + ' ' + %char(%decpos(p2)) + ' ' + %char(%len(p3))") + p("%char(%size(z2)) + ' ' + %char(%len(b2)) + ' ' + %char(%size(b2)) + ' ' + %char(%size(u3))") + ["*inlr = *on;"]

    # LIKE of subfields, array elements, dates and procedures' results.
    M["DCL144"] = F + [
        "dcl-ds rec qualified;",
        "  amt packed(9:3);",
        "  nm char(7);",
        "end-ds;",
        "dcl-s arr int(5) dim(4);",
        "dcl-s dt date(*eur);",
        "dcl-s a like(rec.amt);",
        "dcl-s b like(rec.nm : +1);",
        "dcl-s c like(arr);",
        "dcl-s d like(dt) inz(d'2026-02-28');",
        "dcl-s e like(arr) dim(2);",
    ] + p("%char(%len(a)) + ' ' + %char(%decpos(a)) + ' ' + %char(%len(b)) + ' ' + %char(%size(c)) + ' ' + %char(d) + ' ' + %char(%elem(e))") + ["*inlr = *on;"]

    # LIKE in fixed form with adjustments in the length columns.
    M["DCL145"] = [
        "     H DFTACTGRP(*NO)",
        "     FQSYSPRT   O    F  132        PRINTER",
        "     D line            DS           132",
        "     D BASE            S              9P 3 INZ(1.234)",
        "     D UP              S             +2    LIKE(BASE)",
        "     D DOWN            S             -4    LIKE(BASE)",
        "     D CH              S             10    INZ('ten')",
        "     D CH2             S             -5    LIKE(CH) INZ('five')",
        "     D DS1             DS",
        "     D  SUB                    1      6P 0",
        D(" SUB2", to="+1", kw="LIKE(SUB)"),
        "      /free",
    ] + ff(p("%char(%len(UP)) + ' ' + %char(%decpos(UP)) + ' ' + %char(%len(DOWN)) + ' ' + %char(%len(CH2)) + ' [' + CH2 + '] ' + %char(%len(SUB2)) + ' ' + %char(%size(DS1))")) + [
        "        *inlr = *on;",
        "      /end-free",
    ]

    # Named constants of every form, in expressions and declarations.
    M["DCL146"] = F + [
        "dcl-c MAX 50;",
        "dcl-c RATE 0.075;",
        "dcl-c NAME 'Quartz';",
        "dcl-c HEX x'F0F1';",
        "dcl-c DAY d'2026-12-25';",
        "dcl-c TIM t'10.20.30';",
        "dcl-c STAMP z'2026-12-25-10.20.30.000000';",
        "dcl-c ON *on;",
        "dcl-c UNI u'00510053';",
        "dcl-s buf char(MAX);",
        "dcl-s arr int(5) dim(MAX);",
        "dcl-s amt packed(7:3) inz(RATE);",
    ] + p("%char(%size(buf)) + ' ' + %char(%elem(arr)) + ' ' + %char(amt) + ' ' + NAME + ' ' + HEX") + p("%char(DAY) + ' ' + %char(TIM) + ' ' + %char(STAMP)") + p("%char(UNI) + ' ' + %char(%len(NAME)) + ' ' + %char(MAX * RATE)") + ["*inlr = *on;"]

    # A long named constant continued over lines.
    M["DCL147"] = [
        "     H DFTACTGRP(*NO)",
        "     FQSYSPRT   O    F  132        PRINTER",
        "     D line            DS           132",
        "     D LONG            C                   'The beginning of a long constant +",
        "     D                                     which goes on for a while, then +",
        "     D                                     ends here.'",
        "     D DASH            C                   'dash-",
        "     D                                       continued'",
        "     D NUM             C                   CONST(-123.456)",
        "      /free",
    ] + ff(p("%subst(LONG : 1 : 50)") + p("%subst(LONG : 51) + ' ' + %char(%len(LONG))") + p("'[' + DASH + '] ' + %char(NUM)")) + [
        "        *inlr = *on;",
        "      /end-free",
    ]

    # CONST parameters convert their arguments.
    M["DCL148"] = F + [
        "dcl-pr show varchar(60);",
        "  c char(6) const;",
        "  n packed(7:2) const;",
        "  v varchar(10) const;",
        "  d date const;",
        "end-pr;",
        "dcl-s i int(10) inz(42);",
    ] + p("show('ab' : i : 'xy' + 'z' : d'2026-01-02')") + p("show('abcdef' : 1.255 : '' : %date('2026-03-04'))") + [
        "*inlr = *on;",
        "dcl-proc show;",
        "  dcl-pi *n varchar(60);",
        "    c char(6) const;",
        "    n packed(7:2) const;",
        "    v varchar(10) const;",
        "    d date const;",
        "  end-pi;",
        "  return '[' + c + '] ' + %char(n) + ' [' + v + '] ' + %char(d) + ' ' + %char(%len(v));",
        "end-proc;",
    ]

    # VALUE parameters are copies; a reference parameter is not.
    M["DCL149"] = F + [
        "dcl-pr change;",
        "  byval int(10) value;",
        "  byref int(10);",
        "  ro varchar(5) const;",
        "end-pr;",
        "dcl-s a int(10) inz(1);",
        "dcl-s b int(10) inz(2);",
        "change(a : b : 'ro');",
    ] + p("%char(a) + ' ' + %char(b)") + [
        "*inlr = *on;",
        "dcl-proc change;",
        "  dcl-pi *n;",
        "    byval int(10) value;",
        "    byref int(10);",
        "    ro varchar(5) const;",
        "  end-pi;",
        "  byval = 100;",
        "  byref = 200 + %len(ro);",
        "end-proc;",
    ]

    # Static and automatic locals; a local hides a global.
    M["DCL150"] = F + [
        "dcl-s g int(10) inz(5);",
        "dcl-s i int(10);",
        "for i = 1 to 3;",
    ] + p("%char(bump())", "  ") + [
        "endfor;",
    ] + p("'global ' + %char(g)") + [
        "*inlr = *on;",
        "dcl-proc bump;",
        "  dcl-pi *n int(10) end-pi;",
        "  dcl-s g int(10) inz(1);",
        "  dcl-s s int(10) static inz(10);",
        "  dcl-ds sd static qualified;",
        "    n int(10) inz(100);",
        "  end-ds;",
        "  g += 1;",
        "  s += 1;",
        "  sd.n += 10;",
        "  return g * 10000 + s * 1000 + sd.n;",
        "end-proc;",
    ]

    # Fixed-form structures in every notation.
    M["DCL151"] = [
        "     H DFTACTGRP(*NO)",
        "     FQSYSPRT   O    F  132        PRINTER",
        "     D line            DS           132",
        "     D REC             DS            30    QUALIFIED",
        "     D  A                      1      4",
        "     D  B                      5      8P 2",
        "     D  C                      9     12I 0",
        "     D  D                     13     14U 0",
        "     D  E                     15     18F",
        "     D  F                     19     20B 0",
        "     D  G                     21     23S 1",
        "     D  H                     24     24N",
        "     D  I                     25     30",
        "      /free",
        "        REC.B = -12.34;",
        "        REC.C = 99;",
        "        REC.D = 65535;",
        "        REC.E = 2.5;",
        "        REC.F = -3;",
        "        REC.G = 4.5;",
        "        REC.H = *on;",
    ] + ff(p("%char(REC.B) + ' ' + %char(REC.C) + ' ' + %char(REC.D) + ' ' + %char(REC.E) + ' ' + %char(REC.F) + ' ' + %char(REC.G) + ' ' + REC.H")) + ff(p("%char(%len(REC.B)) + ' ' + %char(%len(REC.C)) + ' ' + %char(%len(REC.D)) + ' ' + %char(%size(REC.E)) + ' ' + %char(%len(REC.F)) + ' ' + %char(%len(REC.G)) + ' ' + %char(%size(REC))")) + [
        "        *inlr = *on;",
        "      /end-free",
    ]

    # Zoned subfields read from character data, packed from hex.
    M["DCL152"] = F + [
        "dcl-ds raw;",
        "  txt char(9) inz('012345678');",
        "  z1 zoned(3:0) overlay(txt);",
        "  z2 zoned(4:2) overlay(txt : 4);",
        "  hx char(3) inz(x'12345F');",
        "  pk packed(5:0) overlay(hx);",
        "  ng char(2) inz(x'012D');",
        "  pn packed(3:1) overlay(ng);",
        "end-ds;",
    ] + p("%char(z1) + ' ' + %char(z2) + ' ' + %char(pk) + ' ' + %char(pn)") + [
        "z2 = -1.5;",
    ] + p("%subst(txt : 1 : 3) + ' ' + %char(z2)") + ["*inlr = *on;"]

    # Integers and binary decimals over character data.
    M["DCL153"] = F + [
        "dcl-ds raw;",
        "  c4 char(4) inz(x'00000102');",
        "  i4 int(10) overlay(c4);",
        "  u2 uns(5) overlay(c4 : 3);",
        "  b2 bindec(4:0) overlay(c4 : 3);",
        "  c8 char(8) inz(x'FFFFFFFFFFFFFFFE');",
        "  i8 int(20) overlay(c8);",
        "  u8 uns(20) overlay(c8);",
        "  i1 int(3) overlay(c8 : 8);",
        "end-ds;",
    ] + p("%char(i4) + ' ' + %char(u2) + ' ' + %char(b2) + ' ' + %char(i8) + ' ' + %char(u8) + ' ' + %char(i1)") + ["*inlr = *on;"]

    # Float values in structures: sizes and round trips.
    M["DCL154"] = F + [
        "dcl-ds fl;",
        "  f4 float(4) inz(0.1);",
        "  f8 float(8) inz(0.1);",
        "  f8b float(8) inz(-1E-5);",
        "end-ds;",
        "dcl-s pk packed(15:7);",
        "pk = f4;",
    ] + p("%char(f4) + ' ' + %char(f8) + ' ' + %char(f8b) + ' ' + %char(pk)") + [
        "pk = f8;",
        "f4 = 1E10;",
    ] + p("%char(pk) + ' ' + %char(f4) + ' ' + %char(%size(fl))") + ["*inlr = *on;"]

    # Indicator subfields and the *IN array as a structure.
    M["DCL155"] = F + [
        "dcl-ds inds len(99);",
        "  exit ind pos(3);",
        "  error ind pos(50);",
        "  range char(3) pos(10);",
        "end-ds;",
        "dcl-s ptr pointer inz(%addr(*in));",
        "dcl-ds ind2 based(ptr);",
        "  in05 ind pos(5);",
        "  in20 ind pos(20);",
        "end-ds;",
        "exit = *on;",
        "range = '101';",
        "*in05 = *on;",
        "in20 = *on;",
    ] + p("%subst(inds : 1 : 12) + ' ' + error + ' ' + in05 + ' ' + *in20 + ' ' + %char(%size(*in))") + ["*inlr = *on;"]

    # Varying fields: the length prefix in a structure.
    M["DCL156"] = F + [
        "dcl-ds vr;",
        "  v2 varchar(6) inz('abc');",
        "  v4 varchar(6 : 4) inz('de');",
        "  ln2 uns(5) pos(1);",
        "end-ds;",
        "dcl-ds raw;",
        "  pre int(5) inz(2);",
        "  dat char(4) inz('wxyz');",
        "  vv varchar(4) pos(1);",
        "end-ds;",
    ] + p("%char(ln2) + ' ' + %char(%size(vr)) + ' ' + %char(%addr(v4) - %addr(vr)) + ' [' + vv + '] ' + %char(%len(vv))") + ["*inlr = *on;"]

    # UCS-2 data: lengths, conversions and initial values.
    M["DCL157"] = F + [
        "dcl-s u1 ucs2(5) inz(u'00680065006C006C006F');",
        "dcl-s u2 varucs2(10) inz(u'00480049');",
        "dcl-s c1 char(5);",
        "dcl-s v1 varchar(10);",
        "c1 = %char(u1);",
        "v1 = %char(u2);",
        "u2 = u2 + %ucs2('!');",
    ] + p("c1 + ' ' + v1 + ' ' + %char(u2) + ' ' + %char(%len(u2)) + ' ' + %char(%size(u1))") + ["*inlr = *on;"]

    # Fixed-form indicator, pointer and date subfields with keywords.
    M["DCL158"] = [
        "     H DFTACTGRP(*NO)",
        "     FQSYSPRT   O    F  132        PRINTER",
        "     D line            DS           132",
        "     D BUF             S             10    INZ('pointed at')",
        "     D DS1             DS                  QUALIFIED INZ",
        "     D  FLAG                           N   INZ(*ON)",
        "     D  PTR                            *   INZ(%ADDR(BUF))",
        "     D  DT                             D   DATFMT(*MDY/)",
        "     D  TM                             T   TIMFMT(*USA)",
        "     D  TS                             Z   INZ(Z'2026-01-01-00.00.00.000000')",
        "     D P2              S               *",
        "     D OVER            S             10    BASED(P2)",
        "      /free",
        "        P2 = DS1.PTR;",
    ] + ff(p("DS1.FLAG + ' ' + OVER + ' ' + %char(DS1.DT) + ' ' + %char(DS1.TM) + ' ' + %char(DS1.TS)") + p("%char(%size(DS1)) + ' ' + %char(%addr(DS1.PTR) - %addr(DS1))")) + [
        "        *inlr = *on;",
        "      /end-free",
    ]

    # Arrays inside structures with POS and DIM, and %SUBARR.
    M["DCL159"] = F + [
        "dcl-ds grid qualified;",
        "  cells zoned(2:0) dim(6) inz(5);",
        "  row2 zoned(2:0) dim(3) pos(7);",
        "end-ds;",
        "dcl-s i int(10);",
        "for i = 1 to 3;",
        "  grid.row2(i) = i * 10;",
        "endfor;",
    ] + p("%char(grid.cells(1)) + ' ' + %char(grid.cells(4)) + ' ' + %char(grid.cells(6)) + ' ' + %char(%xfoot(%subarr(grid.cells : 4 : 3)))") + ["*inlr = *on;"]

    # A structure array: element addresses and sizes.
    M["DCL160"] = F + [
        "dcl-ds e qualified dim(4);",
        "  k int(5);",
        "  v char(3);",
        "end-ds;",
        "dcl-s i int(10);",
        "for i = 1 to %elem(e);",
        "  e(i).k = i * i;",
        "  e(i).v = 'v' + %char(i);",
        "endfor;",
        "sorta(d) e(*).k;",
    ] + p("%char(%size(e)) + ' ' + %char(%size(e : *all)) + ' ' + %char(%addr(e(3)) - %addr(e(1)))") + p("%char(e(4).k) + e(4).v + ' ' + %char(%lookup(9 : e(*).k))") + ["*inlr = *on;"]

    rng = random.Random(161)
    for n in range(161, 201):
        M[f"DCL{n:03d}"] = inz_member(rng, n)


def inz_member(rng, n):
    """Standalone fields of random types and INZ forms, free or fixed,
    with %SIZE, %LEN and the value printed."""
    fixed = n % 4 == 0
    decls, body = [], []
    for i in range(8):
        free, fx, size, kind = sub_shape(rng, allow_ptr=False)
        form = rng.random()
        if form < 0.2:
            inz = "*loval" if kind not in ("varchar", "char", "ucs2", "ind") else ""
        elif form < 0.35:
            inz = "*hival" if kind not in ("varchar", "char", "ucs2", "ind") else ""
        elif form < 0.5:
            inz = "*zeros" if kind in ("num", "float", "char") else ""
        elif form < 0.6:
            inz = "*blanks" if kind in ("char", "varchar") else ""
        else:
            inz = inz_value(rng, kind, free, fx)
        name = f"V{i}"
        dim = rng.random() < 0.2
        if fixed:
            ln, typ, dec, extra = fx
            kws = [extra, "DIM(3)" if dim else "", f"INZ({inz.upper()})" if inz else ""]
            decls += dkw(name, "S", to=str(ln) if ln != "" else "", typ=typ, dec=str(dec) if dec != "" else "", kws=kws)
        else:
            decls.append(f"dcl-s {name} {free}" + (" dim(3)" if dim else "") + (f" inz({inz})" if inz else "") + ";")
        ref = f"{name}(3)" if dim else name
        attrs = f"%char(%size({ref}))"
        if kind in ("num", "char", "varchar", "ucs2"):
            attrs += f" + ' ' + %char(%len({ref}))"
        body += p(f"'{name} ' + {attrs} + ' ' + {value_of(ref, kind)}")
    if fixed:
        return fixed_member(decls, body)
    return FREE_HEAD + decls + body + ["*inlr = *on;"]
