"""Corpus batch 4: operations and built-in functions, free form. Writes
../b04ops/*.rpgle.

usage: python -I b04.py
"""
import os

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b04ops")
HEAD = [
    "**FREE",
    "ctl-opt dftactgrp(*no);",
    "dcl-f QSYSPRT printer(132);",
    "dcl-ds line len(132) end-ds;",
]
M = {}


def member(name, decls, body):
    M[name] = HEAD + decls + body + ["*inlr = *on;"]


def p(expr):
    """Print a character expression: one statement per line."""
    return f"line = {expr};\nwrite QSYSPRT line;"


def wrap(line, width=90):
    """Cut a free-form line at blanks outside literals; the pieces
    continue the statement on the lines after it."""
    if len(line) <= width:
        return [line]
    indent = len(line) - len(line.lstrip())
    out, cur, quote, word = [], "", False, ""
    words = []
    for ch in line.strip():
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
        if len(cand) + indent + 6 > width and cur:
            out.append(cur)
            cur = w
        else:
            cur = cand
    out.append(cur)
    return [" " * indent + out[0]] + [" " * (indent + 6) + x for x in out[1:]]


member("BIF001", [
    "dcl-s s char(12) inz('  ab cd  ');",
    "dcl-s v varchar(20) inz('xxhixx');",
], [
    p("'[' + %trim(s) + ']'"),
    p("'[' + %triml(s) + ']'"),
    p("'[' + %trimr(s) + ']'"),
    p("'[' + %trim(v:'x') + ']'"),
    p("'[' + %triml(v:'x') + ']' + %char(%len(%trimr(v:'x')))"),
    p("'[' + %trim('...a.b...':'.') + ']'"),
    p("'len=' + %char(%len(%trim(s))) + ' ' + %char(%len(s)) + ' ' + %char(%len(v))"),
])

member("BIF002", [
    "dcl-s s char(10) inz('abcdefghij');",
    "dcl-s n int(10) inz(3);",
], [
    p("%subst(s:3:4)"),
    p("%subst(s:8)"),
    p("'[' + %subst(s:n:n) + ']'"),
    p("%char(%scan('d':s)) + ' ' + %char(%scan('z':s)) + ' ' + %char(%scan('c':s:4))"),
    p("%char(%scan('ef':s))"),
    "%subst(s:2:3) = 'XYZ';",
    p("s"),
])

member("BIF003", [
    "dcl-s s char(8) inz('12x45 7');",
    "dcl-c DIGITS '0123456789';",
], [
    p("%char(%check(DIGITS:s)) + ' ' + %char(%checkr(DIGITS + ' ':s))"),
    p("%char(%check(DIGITS:'123')) + ' ' + %char(%check(' ':'   x'))"),
    p("%xlate('abc':'XYZ':'aabbcc')"),
    p("%xlate('0123456789':'##########':s)"),
    p("%xlate('a':'A':'banana':3)"),
])

member("BIF004", [
    "dcl-s s varchar(30) inz('the cat sat');",
], [
    p("%replace('dog':s:5:3)"),
    p("%replace('big ':s:5:0)"),
    p("%replace('':s:4:4)"),
    p("%replace('X':s)"),
    p("%upper('mixed Case') + ' ' + %lower('MIXED Case')"),
])

member("BIF005", [
    "dcl-s p packed(9:2) inz(-1234.5);",
    "dcl-s z zoned(7:0) inz(42);",
    "dcl-s i int(10) inz(-7);",
    "dcl-s u uns(5) inz(65535);",
    "dcl-s f float(8) inz(1.5);",
    "dcl-s q packed(5:5) inz(.00012);",
    "dcl-s zero packed(7:2) inz(0);",
], [
    p("%char(p) + '|' + %char(z) + '|' + %char(i) + '|' + %char(u)"),
    p("%char(q) + '|' + %char(zero) + '|' + %char(-0.5)"),
    p("%char(f)"),
    p("%char(f * 1000000)"),
    p("%char(%dec(p:5:0)) + '|' + %char(%dech(p:5:0)) + '|' + %char(%dec('12.75':5:2))"),
    p("%char(%int(p)) + '|' + %char(%inth(p)) + '|' + %char(%int('-33'))"),
    p("%char(%abs(p)) + '|' + %char(%abs(i))"),
])

member("BIF006", [
    "dcl-s a packed(9:2) inz(1234567.89);",
    "dcl-s b packed(9:2) inz(-1234567.89);",
    "dcl-s c packed(5:0) inz(0);",
    "dcl-s d packed(7:3) inz(-0.25);",
], [
    p("'[' + %editc(a:'1') + '][' + %editc(b:'1') + '][' + %editc(c:'1') + ']'"),
    p("'[' + %editc(a:'2') + '][' + %editc(c:'2') + '][' + %editc(d:'2') + ']'"),
    p("'[' + %editc(a:'3') + '][' + %editc(b:'4') + '][' + %editc(c:'4') + ']'"),
    p("'[' + %editc(b:'A') + '][' + %editc(b:'B') + '][' + %editc(b:'C') + ']'"),
    p("'[' + %editc(b:'J') + '][' + %editc(d:'K') + '][' + %editc(b:'L') + ']'"),
    p("'[' + %editc(b:'N') + '][' + %editc(d:'O') + '][' + %editc(b:'P') + ']'"),
    p("'[' + %editc(b:'X') + '][' + %editc(a:'Z') + '][' + %editc(c:'Z') + ']'"),
    p("'[' + %editc(a:'1':*astfill) + '][' + %editc(a:'J':'$') + ']'"),
])

member("BIF007", [
    "dcl-s amt packed(7:2) inz(1234.56);",
    "dcl-s neg packed(7:2) inz(-12.3);",
    "dcl-s date6 zoned(6:0) inz(102326);",
], [
    p("'[' + %editw(amt:'   ,  0.  ') + ']'"),
    p("'[' + %editw(neg:'   ,  0.  -') + ']'"),
    p("'[' + %editw(neg:'   ,  0.  CR') + ']'"),
    p("'[' + %editw(amt:'$   ,  0.  ') + ']'"),
    p("'[' + %editw(date6:'  /  /  ') + ']'"),
])

member("BIF008", [
    "dcl-s arr packed(5:0) dim(5);",
    "dcl-s c char(7);",
    "dcl-ds ds qualified;",
    "  a char(3);",
    "  b packed(5:2);",
    "end-ds;",
    "dcl-s i int(10);",
], [
    "for i = 1 to %elem(arr);",
    "  arr(i) = i * 10;",
    "endfor;",
    p("%char(%elem(arr)) + ' ' + %char(%size(arr)) + ' ' + %char(%size(arr:*all))"),
    p("%char(%size(c)) + ' ' + %char(%size(ds)) + ' ' + %char(%size(ds.b)) + ' ' + %char(%len(ds.b))"),
    p("%char(%xfoot(arr))"),
    p("%char(%div(17:5)) + ' ' + %char(%rem(17:5)) + ' ' + %char(%div(-17:5)) + ' ' + %char(%rem(-17:5))"),
])

member("BIF009", [
    "dcl-s a packed(5:2) inz(1.5);",
    "dcl-s b packed(5:2) inz(-2.25);",
    "dcl-s c packed(5:2) inz(7);",
], [
    p("%char(%max(a:b:c)) + ' ' + %char(%min(a:b:c))"),
    p("%char(%max(3:9:1)) + ' ' + %char(%min(3:9:1))"),
    p("%max('abc':'abd') + ' ' + %min('b':'a')"),
])

member("OPS001", [
    "dcl-s i int(10);",
    "dcl-s n int(10);",
], [
    "for i = 1 to 10 by 3;",
    "  " + p("'for ' + %char(i)"),
    "endfor;",
    "for i = 10 downto 1 by 4;",
    "  " + p("'down ' + %char(i)"),
    "endfor;",
    "n = 0;",
    "dow n < 3;",
    "  n += 1;",
    "  if n = 2;",
    "    iter;",
    "  endif;",
    "  " + p("'dow ' + %char(n)"),
    "enddo;",
    "dou n >= 6;",
    "  n += 1;",
    "  if n = 5;",
    "    leave;",
    "  endif;",
    "enddo;",
    p("'after ' + %char(n)"),
])

member("OPS002", [
    "dcl-s c char(1);",
    "dcl-s k int(10);",
], [
    "for k = 1 to 5;",
    "  select;",
    "  when k = 1;",
    "    c = 'a';",
    "  when k = 2 or k = 3;",
    "    c = 'b';",
    "  when k > 10;",
    "    c = 'z';",
    "  other;",
    "    c = '?';",
    "  endsl;",
    "  " + p("%char(k) + c"),
    "endfor;",
])

member("OPS003", [
    "dcl-s n int(10);",
    "dcl-s t int(10);",
], [
    "n = 4;",
    "exsr fact;",
    p("'fact ' + %char(t)"),
    "n = 1;",
    "exsr fact;",
    p("'fact ' + %char(t)"),
    "exsr early;",
    "*inlr = *on;",
    "return;",
    "begsr fact;",
    "  t = 1;",
    "  dow n > 1;",
    "    t *= n;",
    "    n -= 1;",
    "  enddo;",
    "endsr;",
    "begsr early;",
    "  " + p("'early'"),
    "  leavesr;",
    "  " + p("'never'"),
    "endsr;",
])

member("OPS004", [
    "dcl-ds rec qualified;",
    "  id packed(5:0) inz(7);",
    "  nm char(5) inz('abc');",
    "  f ind inz(*on);",
    "end-ds;",
    "dcl-s n packed(7:2) inz(3.5);",
    "dcl-s c varchar(10) inz('hey');",
    "dcl-s arr char(2) dim(3) inz('zz');",
], [
    p("%char(rec.id) + rec.nm + rec.f + ' ' + %char(n) + c + arr(2)"),
    "clear rec;",
    "clear n;",
    "clear c;",
    "clear arr;",
    p("%char(rec.id) + '[' + rec.nm + ']' + rec.f + ' ' + %char(n) + '[' + c + ']' + '[' + arr(2) + ']' + %char(%len(c))"),
])

member("OPS005", [
    "dcl-s a ind inz(*on);",
    "dcl-s b ind inz(*off);",
    "dcl-s n int(10) inz(0);",
], [
    "*in01 = *on;",
    "*in(2) = a and b;",
    "*in03 = a or b;",
    "*in04 = not b;",
    p("*in01 + *in02 + *in03 + *in04"),
    "if *in01 and not *in02;",
    "  " + p("'yes'"),
    "endif;",
    "if n = 0 or 1 / n > 1;",
    "  " + p("'short circuit'"),
    "endif;",
    "*in10 = (n > 5);",
    p("'in10=' + *in10"),
])

member("OPS006", [
    "dcl-s c char(10);",
    "dcl-s v varchar(10);",
    "dcl-s n packed(5:2);",
], [
    "c = 'abc';",
    "v = c;",
    p("'[' + v + ']' + %char(%len(v))"),
    "v = %trim(c);",
    p("'[' + v + ']' + %char(%len(v))"),
    "c = 'toolongforfield';",
    p("'[' + c + ']'"),
    "evalr c = 'right';",
    p("'[' + c + ']'"),
    "n = 123.456;",
    p("%char(n)"),
    "eval(h) n = 123.456;",
    p("%char(n)"),
    "eval(h) n = -0.005;",
    p("%char(n)"),
])

member("OPS007", [
    "dcl-s n packed(9:2) inz(10);",
    "dcl-s s varchar(20) inz('a');",
], [
    "n += 5;",
    "n -= 2.5;",
    "n *= 3;",
    "n /= 4;",
    p("%char(n)"),
    "n **= 2;",
    p("%char(n)"),
    "s += 'bc';",
    "s += 'd';",
    p("s + %char(%len(s))"),
])

member("OPS008", [
    "dcl-s x packed(5:0);",
], [
    "monitor;",
    "  x = 1 / 0;",
    "  " + p("'not reached'"),
    "on-error 102;",
    "  " + p("'divide by zero'"),
    "endmon;",
    "monitor;",
    "  x = 99999 + 1;",
    "on-error *program;",
    "  " + p("'program error'"),
    "endmon;",
    "monitor;",
    "  x = 2;",
    "  " + p("'no error'"),
    "on-error;",
    "  " + p("'never'"),
    "endmon;",
    p("'x=' + %char(x)"),
])

for name, lines in M.items():
    flat = []
    for l in "\n".join(lines).split("\n"):
        flat.extend(wrap(l))
    with open(os.path.join(OUT, name + ".rpgle"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(flat) + "\n")
print(len(M), "members")
