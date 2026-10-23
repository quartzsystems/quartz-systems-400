"""Corpus batch 4, dates, times and timestamps. Writes
../b04ops/DTE*.rpgle.

usage: python -I dte.py
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fixed import C, CX, D, H, PRT_D, PRT_F, show  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b04ops")
HEAD = [
    "**FREE",
    "ctl-opt dftactgrp(*no);",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
]
M = {}


def member(name, decls, body, ctl=None, procs=()):
    head = list(HEAD)
    if ctl:
        head[1] = ctl
    M[name] = head + decls + body + ["*inlr = *on;"] + list(procs)


def fixed(name, lines):
    M[name] = lines


def wrap(line, width=90):
    """Cut a free-form line at blanks outside literals; the pieces
    continue the statement on the lines after it."""
    if len(line) <= width:
        return [line]
    out, cur, quote, word, words = [], "", False, "", []
    for ch in line:
        if ch == "'":
            quote = not quote
        if ch == " " and not quote:
            words.append(word)
            word = ""
        else:
            word += ch
    words.append(word)
    for w in words:
        cand = w if not cur else cur + " " + w
        if len(cand) > width and cur:
            out.append(cur)
            cur = w
        else:
            cur = cand
    out.append(cur)
    return [out[0]] + ["      " + x for x in out[1:]]


def p(expr):
    """Print a character expression: one statement per line."""
    return "\n".join(wrap(f"line = {expr};")) + "\nwrite QSYSPRT line;"


def guard(label, stmts, after=None):
    """Run statements in a monitor and print which status ended them."""
    out = ["monitor;"] + ["  " + s for s in stmts]
    out.append("  " + p(f"'{label} ok'"))
    for st in ("112", "113", "114"):
        out.append(f"on-error {st};")
        out.append("  " + p(f"'{label} {st}'"))
    out.append("on-error;")
    out.append("  " + p(f"'{label} other'"))
    out.append("endmon;")
    if after:
        out.append(p(after))
    return out


member("DTE001", [
    "dcl-s d1 date;",
    "dcl-s d2 date(*mdy);",
    "dcl-s d3 date(*jul);",
    "dcl-s d4 date(*usa);",
    "dcl-s t1 time;",
    "dcl-s t2 time(*usa);",
    "dcl-s t3 time(*hms);",
    "dcl-s z1 timestamp;",
    "dcl-s z0 timestamp(0);",
    "dcl-s z3 timestamp(3);",
    "dcl-s z12 timestamp(12);",
], [
    p("%char(d1) + '|' + %char(d2) + '|' + %char(d3) + '|' + %char(d4)"),
    p("%char(t1) + '|' + %char(t2) + '|' + %char(t3)"),
    p("%char(z1) + '|' + %char(z0) + '|' + %char(z3)"),
    p("%char(z12)"),
    "clear d2;",
    "clear t2;",
    "clear z0;",
    p("%char(d2) + '|' + %char(t2) + '|' + %char(z0)"),
])

member("DTE002", [
    "dcl-s d1 date;",
    "dcl-s d2 date(*mdy);",
    "dcl-s d3 date(*jul);",
    "dcl-s t1 time;",
    "dcl-s t2 time(*usa);",
    "dcl-s z1 timestamp;",
    "dcl-s z12 timestamp(12);",
], [
    "d1 = *hival;",
    "d2 = *hival;",
    "d3 = *hival;",
    "t1 = *hival;",
    "t2 = *hival;",
    "z1 = *hival;",
    "z12 = *hival;",
    p("%char(d1) + '|' + %char(d2) + '|' + %char(d3) + '|' + %char(t1) + '|' + %char(t2)"),
    p("%char(z1) + '|' + %char(z12)"),
    "d1 = *loval;",
    "d2 = *loval;",
    "d3 = *loval;",
    "t1 = *loval;",
    "t2 = *loval;",
    "z1 = *loval;",
    p("%char(d1) + '|' + %char(d2) + '|' + %char(d3) + '|' + %char(t1) + '|' + %char(t2)"),
    p("%char(z1)"),
    "if d2 = *loval and d1 < d'1000-01-01';",
    "  " + p("'loval compares'"),
    "endif;",
    "if d1 <> *hival;",
    "  " + p("'not hival'"),
    "endif;",
])

member("DTE003", [
    "dcl-ds raw qualified;",
    "  a date(*iso) inz(d'2026-03-05');",
    "  b date(*usa) inz(d'2026-03-05');",
    "  c date(*eur) inz(d'2026-03-05');",
    "  d date(*jis) inz(d'2026-03-05');",
    "  e date(*mdy-) inz(d'2026-03-05');",
    "  f date(*dmy.) inz(d'2026-03-05');",
    "  g date(*ymd&) inz(d'2026-03-05');",
    "  h date(*jul,) inz(d'2026-03-05');",
    "end-ds;",
    "dcl-ds traw qualified;",
    "  a time(*hms) inz(t'14.30.05');",
    "  b time(*iso) inz(t'14.30.05');",
    "  c time(*usa) inz(t'14.30.05');",
    "  d time(*eur) inz(t'14.30.05');",
    "  e time(*jis) inz(t'14.30.05');",
    "  f time(*hms.) inz(t'14.30.05');",
    "  g time(*hms,) inz(t'14.30.05');",
    "  h time(*hms&) inz(t'14.30.05');",
    "end-ds;",
    "dcl-s z timestamp inz(z'2026-03-05-14.30.05.123456');",
    "dcl-s z0 timestamp(0) inz(z'2026-03-05-14.30.05');",
    "dcl-s z12 timestamp(12) inz(z'2026-03-05-14.30.05.123456789012');",
], [
    p("'[' + raw + ']'"),
    p("'[' + traw + ']'"),
    p("%char(z) + '|' + %char(z0)"),
    p("%char(z12)"),
    p("%char(raw.e) + '|' + %char(raw.g) + '|' + %char(raw.h) + '|' + %char(traw.h)"),
])

member("DTE004", [
    "dcl-s d date inz(d'2026-03-05');",
    "dcl-s e date(*mdy) inz(d'1999-12-31');",
], [
    p("%char(d:*iso) + ' ' + %char(d:*usa) + ' ' + %char(d:*eur) + ' ' + %char(d:*jis)"),
    p("%char(d:*mdy) + ' ' + %char(d:*dmy) + ' ' + %char(d:*ymd) + ' ' + %char(d:*jul)"),
    p("%char(d:*iso0) + ' ' + %char(d:*usa0) + ' ' + %char(d:*eur0) + ' ' + %char(d:*jis0)"),
    p("%char(d:*mdy0) + ' ' + %char(d:*dmy0) + ' ' + %char(d:*ymd0) + ' ' + %char(d:*jul0)"),
    p("%char(d:*cymd) + ' ' + %char(d:*cmdy) + ' ' + %char(d:*cdmy) + ' ' + %char(d:*longjul)"),
    p("%char(d:*cymd0) + ' ' + %char(d:*cmdy0) + ' ' + %char(d:*cdmy0) + ' ' + %char(d:*longjul0)"),
    p("%char(d:*mdy-) + ' ' + %char(d:*dmy.) + ' ' + %char(d:*ymd,) + ' ' + %char(d:*jul&) + '|'"),
    p("%char(e:*cymd) + ' ' + %char(e) + ' ' + %char(e:*iso) + ' ' + %char(e:*cmdy-)"),
    p("'[' + %char(d) + '] ' + %char(%len(%char(e:*cymd)))"),
])

member("DTE005", [
    "dcl-s t time inz(t'14.30.05');",
    "dcl-s u time inz(t'00.05.09');",
    "dcl-s n time inz(t'12.00.00');",
    "dcl-s h time(*hms) inz(t'09.08.07');",
    "dcl-s z timestamp inz(z'2026-03-05-14.30.05.123456');",
    "dcl-s z0 timestamp(0) inz(z'2026-03-05-14.30.05');",
    "dcl-s z12 timestamp(12) inz(z'2026-03-05-14.30.05.123456789012');",
], [
    p("%char(t:*hms) + ' ' + %char(t:*iso) + ' ' + %char(t:*usa) + ' ' + %char(t:*eur) + ' ' + %char(t:*jis)"),
    p("%char(u:*usa) + ' ' + %char(n:*usa) + ' ' + %char(t:*hms0) + ' ' + %char(t:*iso0)"),
    p("%char(t:*hms.) + ' ' + %char(t:*hms,) + ' ' + %char(t:*hms&) + '|' + %char(h)"),
    p("%char(z) + ' ' + %char(z:*iso0)"),
    p("%char(z0) + ' ' + %char(z12)"),
    p("%char(z12:*iso0) + ' ' + %char(z0:*iso0)"),
])

member("DTE006", [
    "dcl-s d date inz(d'2026-03-05');",
    "dcl-s e date(*mdy) inz(d'1999-12-31');",
    "dcl-s t time inz(t'14.30.05');",
    "dcl-s z timestamp inz(z'2026-03-05-14.30.05.123456');",
    "dcl-s z0 timestamp(0) inz(z'2026-03-05-14.30.05');",
    "dcl-s n6 packed(6:0);",
    "dcl-s n8 zoned(8:0);",
], [
    p("%char(%dec(d:*iso)) + ' ' + %char(%dec(d:*usa)) + ' ' + %char(%dec(d:*eur)) + ' ' + %char(%dec(d:*jis))"),
    p("%char(%dec(d:*mdy)) + ' ' + %char(%dec(d:*dmy)) + ' ' + %char(%dec(d:*ymd)) + ' ' + %char(%dec(d:*jul))"),
    p("%char(%dec(d:*cymd)) + ' ' + %char(%dec(d:*cmdy)) + ' ' + %char(%dec(d:*cdmy))"),
    p("%char(%dec(d:*longjul)) + ' ' + %char(%dec(d)) + ' ' + %char(%dec(e)) + ' ' + %char(%dec(e:*cymd))"),
    p("%char(%dec(t:*hms)) + ' ' + %char(%dec(t:*iso)) + ' ' + %char(%dec(t))"),
    p("%char(%dec(z)) + ' ' + %char(%dec(z0))"),
    "n6 = %dec(d:*ymd);",
    "n8 = %dec(e:*usa);",
    p("%char(n6) + ' ' + %char(n8) + ' ' + %char(%dec(d:*ymd) + 1)"),
])

member("DTE007", [
    "dcl-s c10 char(10) inz('2026-03-05');",
    "dcl-s n8 packed(8:0) inz(20260305);",
    "dcl-s n6 zoned(6:0) inz(030526);",
    "dcl-s n7 packed(7:0) inz(1260305);",
    "dcl-s z timestamp inz(z'2026-03-05-14.30.05.123456');",
    "dcl-s d date;",
], [
    p("%char(%date('2026-03-05':*iso)) + ' ' + %char(%date('03/05/2026':*usa))"),
    p("%char(%date('05.03.2026':*eur)) + ' ' + %char(%date('26/03/05':*ymd))"),
    p("%char(%date('26/064':*jul)) + ' ' + %char(%date('20260305':*iso0))"),
    p("%char(%date('030526':*mdy0)) + ' ' + %char(%date('126/03/05':*cymd))"),
    p("%char(%date('1260305':*cymd0)) + ' ' + %char(%date('2026/064':*longjul))"),
    p("%char(%date(n8:*iso)) + ' ' + %char(%date(n6:*mdy)) + ' ' + %char(%date(n7:*cymd))"),
    p("%char(%date(26064:*jul)) + ' ' + %char(%date(2026064:*longjul)) + ' ' + %char(%date(n8))"),
    p("%char(%date(c10)) + ' ' + %char(%date(z)) + ' ' + %char(%date('991231':*ymd0))"),
    p("%char(%date('400101':*ymd0)) + ' ' + %char(%date('391231':*ymd0))"),
    p("%char(%date('2026-03-05':*iso):*mdy) + ' ' + %char(%date(1991231:*cymd))"),
    "d = %date('2026-03-05':*iso);",
    "if %date(%char(d:*jobrun):*jobrun) = d;",
    "  " + p("'jobrun round trip'"),
    "endif;",
])

member("DTE008", [
    "dcl-s n6 zoned(6:0) inz(143005);",
    "dcl-s n20 packed(20:0) inz(20260305143005123456);",
], [
    p("%char(%time('14.30.05':*iso)) + ' ' + %char(%time('14:30:05':*hms))"),
    p("%char(%time('02:30 PM':*usa)) + ' ' + %char(%time('12:15 AM':*usa))"),
    p("%char(%time(n6:*hms)) + ' ' + %char(%time('143005':*hms0)) + ' ' + %char(%time(143005))"),
    p("%char(%time(z'2026-03-05-14.30.05.123456'))"),
    p("%char(%timestamp('2026-03-05-14.30.05.123456'))"),
    p("%char(%timestamp(n20)) + ' ' + %char(%timestamp(d'2026-03-05'))"),
    p("%char(%timestamp('20260305143005123456':*iso0))"),
])

member("DTE009", [
    "dcl-s d date;",
    "dcl-s e date(*mdy) inz(d'1999-12-31');",
    "dcl-s n int(10) inz(45);",
], [
    "d = d'2024-01-31' + %months(1);",
    p("%char(d) + ' ' + %char(d'2024-02-29' + %years(1)) + ' ' + %char(d'2024-02-29' - %years(4))"),
    p("%char(d'2026-03-31' - %months(1)) + ' ' + %char(d'2026-01-01' + %days(365))"),
    p("%char(d'2026-01-01' - %days(1)) + ' ' + %char(d'2026-03-05' + %days(-10))"),
    "d = d'2026-03-05';",
    "d += %days(30);",
    p("%char(d)"),
    "d -= %months(13);",
    p("%char(d)"),
    "d = d + %days(n) - %months(2) + %years(1);",
    p("%char(d)"),
    "e = e + %days(1);",
    p("%char(e) + ' ' + %char(e - %months(25)) + ' ' + %char(e + %days(n * 2))"),
    p("%char(d'2026-05-31' + %months(1)) + ' ' + %char(d'2026-12-31' + %months(2))"),
])

member("DTE010", [
    "dcl-s e date(*mdy) inz(d'2026-03-05');",
], [
    p("%char(%diff(d'2026-03-05':d'2025-03-05':*days)) + ' ' + %char(%diff(d'2026-03-05':d'2025-03-05':*months))"),
    p("%char(%diff(d'2026-03-05':d'2025-03-05':*years)) + ' ' + %char(%diff(d'2026-03-04':d'2025-03-05':*years))"),
    p("%char(%diff(d'2025-03-05':d'2026-03-04':*months)) + ' ' + %char(%diff(d'2025-03-05':d'2026-03-04':*y))"),
    p("%char(%diff(d'2024-02-29':d'2024-01-31':*months)) + ' ' + %char(%diff(d'2024-03-31':d'2024-02-29':*m))"),
    p("%char(%diff(d'2024-01-31':d'2024-02-29':*months)) + ' ' + %char(%diff(d'2025-02-28':d'2024-02-29':*y))"),
    p("%char(%diff(d'2026-01-01':d'1940-01-01':*days)) + ' ' + %char(%diff(e:d'2026-01-01':*d))"),
    p("%char(%diff(d'2026-01-01':d'2026-03-05':*days)) + ' ' + %char(%diff(e:d'2025-03-06':*years))"),
])

member("DTE011", [
    "dcl-s t time inz(t'23.30.00');",
    "dcl-s u time(*usa) inz(t'14.00.00');",
], [
    p("%char(t + %hours(1)) + ' ' + %char(t + %minutes(45)) + ' ' + %char(t - %seconds(3601))"),
    p("%char(t + %hours(25)) + ' ' + %char(t - %hours(24)) + ' ' + %char(u + %minutes(90))"),
    p("%char(%diff(t'14.00.00':t'09.30.15':*hours)) + ' ' + %char(%diff(t'14.00.00':t'09.30.15':*minutes))"),
    p("%char(%diff(t'14.00.00':t'09.30.15':*seconds)) + ' ' + %char(%diff(t'09.00.00':u:*hours))"),
    p("%char(%diff(t'09.00.00':t'14.00.01':*h)) + ' ' + %char(%diff(t'09.00.00':t'14.00.01':*mn))"),
    p("%char(%subdt(t:*hours)) + ' ' + %char(%subdt(t:*minutes)) + ' ' + %char(%subdt(u:*seconds))"),
    p("%char(%subdt(u:*h)) + ' ' + %char(%subdt(t:*mn)) + ' ' + %char(%subdt(t:*s))"),
    "t += %seconds(59);",
    p("%char(t)"),
])

member("DTE012", [
    "dcl-s z timestamp inz(z'2026-03-05-23.59.59.999999');",
    "dcl-s y timestamp inz(z'2025-01-31-08.00.00.000000');",
    "dcl-s r timestamp;",
], [
    "r = z + %mseconds(1);",
    p("%char(r) + ' ' + %char(z + %days(1))"),
    p("%char(y + %months(1)) + ' ' + %char(y - %years(1))"),
    p("%char(y + %hours(2)) + ' ' + %char(y - %minutes(1)) + ' ' + %char(y + %seconds(90))"),
    p("%char(y - %mseconds(1500000))"),
    p("%char(%diff(z:y:*ms)) + ' ' + %char(%diff(z:y:*seconds)) + ' ' + %char(%diff(z:y:*minutes))"),
    p("%char(%diff(z:y:*hours)) + ' ' + %char(%diff(z:y:*days)) + ' ' + %char(%diff(z:y:*months))"),
    p("%char(%diff(z:y:*years)) + ' ' + %char(%diff(y:z:*days)) + ' ' + %char(%diff(y:z:*mseconds))"),
    p("%char(%subdt(z:*years)) + ' ' + %char(%subdt(z:*months)) + ' ' + %char(%subdt(z:*days))"),
    p("%char(%subdt(z:*hours)) + ' ' + %char(%subdt(z:*minutes)) + ' ' + %char(%subdt(z:*seconds))"),
    p("%char(%subdt(z:*mseconds)) + ' ' + %char(%subdt(z:*ms)) + ' ' + %char(%subdt(y:*d))"),
])

member("DTE013", [
    "dcl-s d date inz(d'2026-03-05');",
    "dcl-s e date(*mdy) inz(d'2026-03-05');",
    "dcl-s j date(*jul) inz(d'1999-12-31');",
    "dcl-s n packed(9:0);",
], [
    p("%char(%subdt(d:*years)) + ' ' + %char(%subdt(d:*months)) + ' ' + %char(%subdt(d:*days))"),
    p("%char(%subdt(e:*y)) + ' ' + %char(%subdt(e:*m)) + ' ' + %char(%subdt(e:*d))"),
    p("%char(%subdt(j:*y)) + ' ' + %char(%subdt(j:*m)) + ' ' + %char(%subdt(j:*d))"),
    "n = %subdt(d:*y) * 10000 + %subdt(d:*m) * 100 + %subdt(d:*d);",
    p("%char(n)"),
    p("%char(%subdt(d'2024-02-29' + %days(1):*days)) + ' ' + %char(%subdt(z'2026-03-05-01.02.03':*mn))"),
])

member("DTE014", [
    "dcl-s d date inz(d'2026-03-05');",
    "dcl-s e date(*mdy) inz(d'2026-03-05');",
    "dcl-s f date(*jul) inz(d'2026-03-06');",
    "dcl-s t time inz(t'14.30.00');",
    "dcl-s u time(*usa) inz(t'14.30.00');",
    "dcl-s z timestamp inz(z'2026-03-05-14.30.00.000000');",
    "dcl-s z0 timestamp(0) inz(z'2026-03-05-14.30.00');",
    "dcl-s z3 timestamp(3) inz(z'2026-03-05-14.30.00.001');",
], [
    "if d = e;",
    "  " + p("'d = e'"),
    "endif;",
    "if e < f and f > d and d <= e and f >= e and e <> f;",
    "  " + p("'order ok'"),
    "endif;",
    "if t = u and u >= t'14.29.59' and u < t'14.31.00';",
    "  " + p("'times ok'"),
    "endif;",
    "if z = z0 and z3 > z and z0 < z3;",
    "  " + p("'stamps ok'"),
    "endif;",
    "if d > *loval and d < *hival and e = d'2026-03-05';",
    "  " + p("'figuratives ok'"),
    "endif;",
    "select;",
    "when f < d'2026-01-01';",
    "  " + p("'q0'"),
    "when f < d'2026-04-01';",
    "  " + p("'q1'"),
    "other;",
    "  " + p("'later'"),
    "endsl;",
    p("%char(%max(d:f)) + ' ' + %char(%min(e:f))"),
])

member("DTE015", [
    "dcl-s d date inz(d'2026-03-05');",
    "dcl-s e date(*mdy);",
    "dcl-s f date(*jul);",
    "dcl-s g date(*eur);",
    "dcl-s t time inz(t'14.30.05');",
    "dcl-s u time(*usa);",
    "dcl-s h time(*hms);",
    "dcl-s z timestamp inz(z'2026-03-05-14.30.05.123456');",
    "dcl-s z0 timestamp(0);",
    "dcl-s z12 timestamp(12);",
    "dcl-s z3 timestamp(3);",
    "dcl-s c char(12);",
], [
    "e = d;",
    "f = e;",
    "g = f;",
    "d = d'1999-01-01';",
    "d = g;",
    p("%char(e) + ' ' + %char(f) + ' ' + %char(g) + ' ' + %char(d)"),
    "u = t;",
    "h = u;",
    p("%char(u) + ' ' + %char(h)"),
    "z0 = z;",
    "z12 = z;",
    "z3 = z;",
    p("%char(z0) + ' ' + %char(z3)"),
    p("%char(z12)"),
    "z = z0;",
    p("%char(z)"),
    "c = %char(e);",
    p("'[' + c + ']'"),
    "d = %date(z12);",
    "t = %time(z12);",
    p("%char(d) + ' ' + %char(t)"),
])

member("DTE016", [
    "dcl-s d date inz(d'2026-03-05');",
    "dcl-s e date(*mdy) inz(d'2026-03-05');",
    "dcl-s t time inz(t'14.30.05');",
    "dcl-s z timestamp;",
    "dcl-s c char(10) inz('2026-02-30');",
    "dcl-s n packed(8:0) inz(20261301);",
    "dcl-s x char(2) inz('xx');",
], [
    *guard("bad day", ["d = %date(c:*iso);"], "%char(d)"),
    *guard("bad text", ["d = %date(x:*iso);"]),
    *guard("bad num", ["d = %date(n:*iso);"]),
    *guard("mdy range", ["e = d'2050-01-01';"], "%char(e)"),
    *guard("mdy add", ["e = e + %years(30);"], "%char(e)"),
    *guard("top", ["d = d'9999-12-31' + %days(1);"], "%char(d)"),
    *guard("bottom", ["d = d'0001-01-01' - %days(1);"]),
    *guard("bad time", ["t = %time('25.00.00':*iso);"], "%char(t)"),
    *guard("bad stamp", ["z = %timestamp('2026-03-05-24.00.00.000000');"]),
    *guard("bad sep", ["d = %date('2026/03/05':*iso);"], "%char(d)"),
    *guard("mdy sep", ["e = %date('03-05-26':*mdy);"], "%char(e)"),
])

member("DTE017", [
    "dcl-s d date;",
    "dcl-s e date(*mdy) inz(d'2000-02-29');",
    "dcl-s t time inz(t'01.02.03');",
    "dcl-s z timestamp inz(z'2026-03-05-14.30.05.123456');",
    "dcl-s y timestamp(3);",
], [
    "d = %date(z) + %days(%diff(z:z'2026-01-01-00.00.00':*days));",
    p("%char(d) + ' ' + %char(%diff(d:e:*years) + 1)"),
    "y = z + %days(1) + %hours(1) + %minutes(1) + %seconds(1) + %mseconds(1000);",
    p("%char(y)"),
    "e = e + %years(1);",
    p("%char(e) + ' ' + %char(e - %years(1)) + ' ' + %char(d'2000-02-29' + %years(100))"),
    p("%char(%timestamp(%date(z)) + %hours(%subdt(t:*h)))"),
])

member("DTE018", [
    "dcl-ds ds qualified;",
    "  d date(*iso) inz(d'2026-03-05');",
    "  t time(*hms) inz(t'14.30.05');",
    "  z timestamp(0) inz(z'2026-03-05-14.30.05');",
    "end-ds;",
    "dcl-s list date dim(3);",
    "dcl-s i int(10);",
], [
    "list(1) = d'2026-01-01';",
    "for i = 2 to 3;",
    "  list(i) = list(i - 1) + %months(i);",
    "endfor;",
    p("%char(list(1)) + ' ' + %char(list(2)) + ' ' + %char(list(3))"),
    "ds.d += %days(1);",
    "ds.t -= %hours(15);",
    "ds.z += %years(1);",
    p("'[' + ds + ']'"),
    p("%char(ds.d:*mdy) + ' ' + %char(ds.t:*usa) + ' ' + %char(ds.z)"),
])

member("DTE019", [
    "dcl-s d date inz(d'2026-03-05');",
    "dcl-s e date inz(*loval);",
    "dcl-s t time;",
], [
    "e = shift(d:10);",
    p("%char(e) + ' ' + %char(shift(d'2026-12-25':7))"),
    "t = later(t'10.00.00');",
    p("%char(t) + ' ' + %char(span(d:e))"),
], procs=[
    "dcl-proc shift;",
    "  dcl-pi *n date;",
    "    from date(*mdy) const;",
    "    n int(10) value;",
    "  end-pi;",
    "  return from + %days(n);",
    "end-proc;",
    "dcl-proc later;",
    "  dcl-pi *n time(*usa);",
    "    t time value;",
    "  end-pi;",
    "  return t + %minutes(75);",
    "end-proc;",
    "dcl-proc span;",
    "  dcl-pi *n int(10);",
    "    a date const;",
    "    b date(*jul) const;",
    "  end-pi;",
    "  return %diff(b:a:*days);",
    "end-proc;",
])

# Control-specification defaults.
member("DTE020", [
    "dcl-s d date inz(d'05.03.26');",
    "dcl-s t time inz(t'14,30,05');",
    "dcl-s e date(*iso);",
    "dcl-ds raw;",
    "  rd date inz(d'05.03.26');",
    "  rt time inz(t'14,30,05');",
    "end-ds;",
], [
    p("%char(d) + ' ' + %char(t) + ' ' + raw"),
    "e = %date('05.03.26');",
    p("%char(e) + ' ' + %char(%date(050326)) + ' ' + %char(%time('14,31,06'))"),
    p("%char(%dec(d)) + ' ' + %char(%dec(t))"),
], ctl="ctl-opt dftactgrp(*no) datfmt(*dmy.) timfmt(*hms,);")

# What the compiler refuses: formats a built-in does not take and
# literals too short for their format.
member("DTE026", [
    "dcl-s t time inz(t'14.30.05');",
    "dcl-s d date;",
    "dcl-s z timestamp;",
], [
    p("%char(t:*usa0)"),
    "d = %date('26-3-5':*ymd);",
    "z = %timestamp('2026-03-05-14.30.05');",
    p("%char(d) + %char(z)"),
])

# Timestamp precision.
member("DTE021", [
    "dcl-s z0 timestamp(0) inz(z'2026-03-05-14.30.05');",
    "dcl-s z1 timestamp(1) inz(z'2026-03-05-14.30.05.9');",
    "dcl-s z9 timestamp(9) inz(z'2026-03-05-14.30.05.123456789');",
    "dcl-s z12 timestamp(12) inz(z'2026-03-05-14.30.05.000000000001');",
    "dcl-s z timestamp;",
], [
    p("%char(z0) + ' ' + %char(z1) + ' ' + %char(z9)"),
    p("%char(z12) + ' ' + %char(%len(%char(z12))) + ' ' + %char(%size(z9))"),
    "z = z9;",
    p("%char(z) + ' ' + %char(%subdt(z9:*ms)) + ' ' + %char(%subdt(z12:*ms))"),
    p("%char(%diff(z12:z0:*ms)) + ' ' + %char(%diff(z9:z1:*ms))"),
    p("%char(z12 + %mseconds(1)) + ' ' + %char(z1 + %mseconds(100000))"),
    "if z12 > z0 and z1 > z9;",
    "  " + p("'precision compares'"),
    "endif;",
    "z9 = z12;",
    p("%char(z9)"),
])

# The current date and time, tested only indirectly.
member("DTE022", [
    "dcl-s d date;",
    "dcl-s t time;",
    "dcl-s z timestamp;",
    "dcl-s ds date inz(*sys);",
    "dcl-s dj date inz(*job);",
    "dcl-s ts time inz(*sys);",
    "dcl-s zs timestamp inz(*sys);",
    "dcl-s e date(*mdy);",
], [
    "d = %date();",
    "t = %time();",
    "z = %timestamp();",
    "e = %date();",
    "if d > d'2026-01-01' and %diff(d:ds:*days) <= 1 and e = d;",
    "  " + p("'date ok'"),
    "endif;",
    "if dj > d'2026-01-01' and dj <= d;",
    "  " + p("'job date ok'"),
    "endif;",
    "if t >= t'00.00.00' and %abs(%diff(t:ts:*seconds)) < 86400;",
    "  " + p("'time ok'"),
    "endif;",
    "if z > z'2026-01-01-00.00.00' and %diff(%timestamp():z:*seconds) < 60;",
    "  " + p("'stamp ok'"),
    "endif;",
    "if zs <= z and %date(z) >= ds;",
    "  " + p("'inz sys ok'"),
    "endif;",
])

# Fixed form: D, T and Z definitions, ADDDUR, SUBDUR, EXTRCT.
fixed("DTE023", [
    H("DFTACTGRP(*NO)"),
    PRT_F,
    PRT_D,
    D("DATE1", "S", typ="D", kw="DATFMT(*USA) INZ(D'2026-03-05')"),
    D("DATE2", "S", typ="D"),
    D("DATE3", "S", typ="D", kw="DATFMT(*YMD)"),
    D("TIME1", "S", typ="T", kw="TIMFMT(*HMS) INZ(T'14.30.05')"),
    D("TIME2", "S", typ="T"),
    D("STAMP1", "S", typ="Z", kw="INZ(Z'2026-03-05-14.30.05.123456')"),
    D("STAMP2", "S", typ="Z"),
    D("NUM", "S", to="9", typ="P", dec="0"),
    D("NDAYS", "S", to="5", typ="P", dec="0", kw="INZ(10)"),
    C("DATE1", "ADDDUR", "5:*D", "DATE2"),
    *show("%CHAR(DATE2)"),
    C(op="ADDDUR", f2="1:*M", res="DATE1"),
    *show("%CHAR(DATE1)"),
    C("DATE1", "SUBDUR", "DATE2", "NUM:*D"),
    *show("%CHAR(NUM)"),
    C("DATE2", "SUBDUR", "NDAYS:*DAYS", "DATE3"),
    *show("%CHAR(DATE3)"),
    C(op="SUBDUR", f2="2:*Y", res="DATE3"),
    *show("%CHAR(DATE3)"),
    C("DATE1", "SUBDUR", "DATE3", "NUM:*M"),
    *show("%CHAR(NUM)"),
    C("TIME1", "ADDDUR", "90:*MN", "TIME2"),
    *show("%CHAR(TIME2)"),
    C("TIME2", "SUBDUR", "TIME1", "NUM:*S"),
    *show("%CHAR(NUM)"),
    C("STAMP1", "ADDDUR", "1:*MS", "STAMP2"),
    *show("%CHAR(STAMP2)"),
    C(op="ADDDUR", f2="-36:*H", res="STAMP2"),
    *show("%CHAR(STAMP2)"),
    C("STAMP1", "SUBDUR", "STAMP2", "NUM:*H"),
    *show("%CHAR(NUM)"),
    C(op="EXTRCT", f2="DATE1:*Y", res="NUM"),
    *show("%CHAR(NUM)"),
    C(op="EXTRCT", f2="DATE1:*M", res="NUM"),
    *show("%CHAR(NUM)"),
    C(op="EXTRCT", f2="TIME1:*MN", res="NUM"),
    *show("%CHAR(NUM)"),
    C(op="EXTRCT", f2="STAMP1:*MS", res="NUM"),
    *show("%CHAR(NUM)"),
    C(op="SETON", hi="LR"),
])

# Fixed form: TEST and its error indicator.
fixed("DTE024", [
    H("DFTACTGRP(*NO) DATFMT(*YMD)"),
    PRT_F,
    PRT_D,
    D("CHR10", "S", to="10", kw="INZ('2026-03-05')"),
    D("CHR8", "S", to="8", kw="INZ('26/02/30')"),
    D("CHR6", "S", to="6", kw="INZ('260228')"),
    D("NUM6", "S", to="6", typ="S", dec="0", kw="INZ(022826)"),
    D("NUM7", "S", to="7", typ="P", dec="0", kw="INZ(1260229)"),
    D("TCHR", "S", to="8", kw="INZ('14:30:61')"),
    D("ZCHR", "S", to="26", kw="INZ('2026-03-05-14.30.05.123456')"),
    D("BAD", "DS"),
    D("BADDATE", "", typ="D", kw="DATFMT(*ISO)"),
    D("BADCHR", "", to="10", kw="OVERLAY(BAD)"),
    C("*ISO", "TEST(D)", res="CHR10", lo="50"),
    *show("'iso ' + *IN50"),
    C(op="TEST(D)", res="CHR8", lo="51"),
    *show("'ymd ' + *IN51"),
    C("*YMD0", "TEST(D)", res="CHR6", lo="52"),
    *show("'ymd0 ' + *IN52"),
    C("*MDY", "TEST(D)", res="NUM6", lo="53"),
    *show("'mdy num ' + *IN53"),
    C("*CYMD", "TEST(D)", res="NUM7", lo="54"),
    *show("'cymd num ' + *IN54"),
    C("*HMS", "TEST(T)", res="TCHR", lo="55"),
    *show("'hms ' + *IN55"),
    C(op="TEST(Z)", res="ZCHR", lo="56"),
    *show("'stamp ' + *IN56"),
    CX("EVAL", "BADCHR = '2026-13-01'"),
    C(op="TEST", res="BADDATE", lo="57"),
    *show("'field ' + *IN57"),
    CX("EVAL", "BADCHR = '2026-12-01'"),
    C(op="TEST", res="BADDATE", lo="58"),
    *show("'field ' + *IN58"),
    C(op="SETON", hi="LR"),
])

# Fixed form: TIME into numeric, time, date and timestamp fields.
fixed("DTE025", [
    H("DFTACTGRP(*NO)"),
    PRT_F,
    PRT_D,
    D("N6", "S", to="6", typ="S", dec="0"),
    D("N12", "S", to="12", typ="P", dec="0"),
    D("N14", "S", to="14", typ="P", dec="0"),
    D("TM", "S", typ="T"),
    D("DT", "S", typ="D"),
    D("TS", "S", typ="Z"),
    C(op="TIME", res="N6"),
    C(op="TIME", res="N12"),
    C(op="TIME", res="N14"),
    C(op="TIME", res="TM"),
    C(op="TIME", res="DT"),
    C(op="TIME", res="TS"),
    CX("IF", "N6 >= 0 AND N6 <= 235959 AND %DIV(N12:1000000) = N6"),
    *show("'numeric ok'"),
    C(op="ENDIF"),
    CX("IF", "%DIV(N14:100000000) = N6 AND %ABS(%DIFF(%TIME():TM:*S)) < 60"),
    *show("'time ok'"),
    C(op="ENDIF"),
    CX("IF", "%DIFF(%DATE():DT:*D) <= 1 AND %DIFF(%TIMESTAMP():TS:*S) < 60"),
    *show("'date ok'"),
    C(op="ENDIF"),
    C(op="SETON", hi="LR"),
])

# DTE016 and DTE020 as the oracle compiles them: a field long enough
# for its format, and explicit formats where *ISO is the default.
M["DTE027"] = [ln.replace("dcl-s x char(2) inz('xx');", "dcl-s x char(10) inz('xx');") for ln in M["DTE016"]]
M["DTE028"] = [
    ln.replace("%date('05.03.26')", "%date('05.03.26':*dmy.)").replace("%date(050326)", "%date(050326:*dmy)").replace("%time('14,31,06')", "%time('14,31,06':*hms,)")
    for ln in M["DTE020"]
]

if __name__ == "__main__":
    for name, lines in M.items():
        text = "\n".join(lines) + "\n"
        for ln in text.split("\n"):
            assert len(ln) <= 100, (name, ln)
        with open(os.path.join(OUT, name + ".rpgle"), "w", newline="\n") as f:
            f.write(text)
    print(len(M), "members")
