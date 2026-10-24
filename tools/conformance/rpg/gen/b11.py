"""Corpus batch 11: embedded SQL (CRTSQLRPGI). Writes ../b11sql/*.rpgle;
the tables come from ../dds/EMPT.sql and ../dds/DEPTT.sql.

usage: python b11.py
"""
import os

from fixed import C, CX, D, comment, place, PRT_F, PRT_D

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "b11sql")

CMD = "CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)"
SC = "%char(sqlcode) + ' ' + sqlstate"


def head(*files, cmd=CMD, ctl="ctl-opt dftactgrp(*no);", extra=()):
    out = ["**FREE", f"//; cmd: {cmd}"]
    out += list(extra)
    for f in files:
        out.append(f"//; sqlfile: {f}")
    out += [ctl, "dcl-f QSYSPRT printer(132);", "dcl-ds line len(132) end-ds;"]
    return out


def P(expr):
    return [f"line = {expr};", "write QSYSPRT line;"]


def fhead(*files, extra=()):
    out = [place([(6, "H"), (7, "DFTACTGRP(*NO)")]), f"      *; cmd: {CMD}"]
    out += [f"      *; {e[4:]}" for e in extra]
    for f in files:
        out.append(f"      *; sqlfile: {f}")
    return out + [PRT_F, PRT_D]


def sql(*lines):
    """A fixed-form SQL block: C/EXEC SQL, C+ lines, C/END-EXEC."""
    out = [place([(6, "C"), (7, "/EXEC SQL " + lines[0])])]
    out += [place([(6, "C"), (7, "+ " + l)]) for l in lines[1:]]
    out.append(place([(6, "C"), (7, "/END-EXEC")]))
    return out


def fshow(expr):
    return [CX("EVAL", f"LINE = {expr}"), C(op="WRITE", f2="QSYSPRT", res="LINE")]


END = ["*inlr = *on;"]
M = {}

M["SQL001"] = head("EMPT") + [
    "dcl-s nm varchar(20);",
    "dcl-s dp char(3);",
    "dcl-s sal packed(9:2);",
    "dcl-s id int(10) inz(3);",
    "// SELECT INTO: one row, no row, more than one row, aggregates.",
    "exec sql select NAME, DEPT, SAL into :nm, :dp, :sal from EMPT where ID = :id;",
] + P(SC + " + ' ' + nm + ' ' + dp + ' ' + %char(sal)") + [
    "id = 99;",
    "exec sql select NAME into :nm from EMPT where ID = :id;",
] + P(SC + " + ' ' + nm + ' ' + %char(sqlerrd(3))") + [
    "exec sql select NAME into :nm from EMPT where DEPT = 'DEV';",
] + P(SC + " + ' ' + nm + ' ' + %char(sqlerrd(3))") + [
    "exec sql select count(*), sum(SAL), max(NAME) into :id, :sal, :nm from EMPT;",
] + P(SC + " + ' ' + %char(id) + ' ' + %char(sal) + ' ' + nm") + END

M["SQL002"] = head("EMPT") + [
    "dcl-s bon packed(7:2);",
    "dcl-s ib int(5);",
    "dcl-s id int(10);",
    "// Null indicators: :v :i, INDICATOR :i, a null without one.",
    "for id = 1 to 5;",
    "  exec sql select BONUS into :bon :ib from EMPT where ID = :id;",
] + ["  " + l for l in P(SC + " + ' ' + %char(id) + ' ' + %char(bon) + ' ' + %char(ib)")] + [
    "endfor;",
    "bon = 9;",
    "exec sql select BONUS into :bon indicator :ib from EMPT where ID = 2;",
] + P(SC + " + ' ' + %char(bon) + ' ' + %char(ib)") + [
    "bon = 7;",
    "exec sql select BONUS into :bon from EMPT where ID = 4;",
] + P(SC + " + ' ' + %char(bon)") + [
    "ib = -1;",
    "exec sql update EMPT set BONUS = :bon :ib where ID = 1;",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "exec sql select count(*) into :id from EMPT where BONUS is null;",
] + P(SC + " + ' ' + %char(id)") + END

M["SQL003"] = head("EMPT") + [
    "dcl-s id int(10);",
    "dcl-s nm varchar(20);",
    "// A cursor: fetch before open, open twice, the loop, close twice.",
    "exec sql declare c1 cursor for select ID, NAME from EMPT where SAL > 50000 order by SAL desc;",
    "exec sql fetch c1 into :id, :nm;",
] + P(SC) + [
    "exec sql open c1;",
] + P(SC) + [
    "exec sql open c1;",
] + P(SC) + [
    "dow 1 = 1;",
    "  exec sql fetch c1 into :id, :nm;",
    "  if sqlcode <> 0;",
    "    leave;",
    "  endif;",
] + ["  " + l for l in P("%char(id) + ' ' + nm + ' ' + %char(sqlerrd(3))")] + [
    "enddo;",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "exec sql close c1;",
] + P(SC) + [
    "exec sql close c1;",
] + P(SC) + END

M["SQL004"] = head("EMPT") + [
    "dcl-s id int(10) inz(6);",
    "dcl-s nm varchar(20) inz('Fay');",
    "dcl-s dp char(3) inz('DEV');",
    "dcl-s sal packed(9:2) inz(45000.5);",
    "dcl-s n int(10);",
    "// INSERT: host variables, a duplicate key, two rows, a null.",
    "exec sql insert into EMPT (ID, NAME, DEPT, SAL) values (:id, :nm, :dp, :sal);",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "exec sql insert into EMPT (ID, NAME, DEPT, SAL) values (:id, :nm, :dp, :sal);",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "exec sql insert into EMPT (ID, NAME) values (7, 'Gus'), (8, 'Hal');",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "exec sql select count(*), sum(SAL) into :n, :sal from EMPT;",
] + P(SC + " + ' ' + %char(n) + ' ' + %char(sal)") + [
    "exec sql insert into EMPT (ID, NAME) values (9, NULL);",
] + P(SC) + END

M["SQL005"] = head("EMPT") + [
    "dcl-s pct packed(3:2) inz(1.10);",
    "dcl-s dp char(3) inz('DEV');",
    "dcl-s sal packed(11:2);",
    "dcl-s n int(10);",
    "// Searched UPDATE and DELETE; no row is +100.",
    "exec sql update EMPT set SAL = SAL * :pct where DEPT = :dp;",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "exec sql update EMPT set SAL = 0 where DEPT = 'XXX';",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "exec sql select sum(SAL) into :sal from EMPT where DEPT = :dp;",
] + P(SC + " + ' ' + %char(sal)") + [
    "exec sql delete from EMPT where DEPT = 'ACC';",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "exec sql delete from EMPT where DEPT = 'ACC';",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "exec sql select count(*) into :n from EMPT;",
] + P(SC + " + ' ' + %char(n)") + END

M["SQL006"] = head("EMPT") + [
    "dcl-s id int(10);",
    "dcl-s sal packed(9:2);",
    "// Positioned UPDATE and DELETE through a FOR UPDATE cursor.",
    "exec sql declare c1 cursor for select ID, SAL from EMPT for update of SAL;",
    "exec sql declare c2 cursor for select ID, SAL from EMPT order by ID;",
    "exec sql open c1;",
    "dow 1 = 1;",
    "  exec sql fetch c1 into :id, :sal;",
    "  if sqlcode <> 0;",
    "    leave;",
    "  endif;",
    "  if id = 2;",
    "    exec sql update EMPT set SAL = SAL + 1000 where current of c1;",
] + ["    " + l for l in P("'upd ' + " + SC + " + ' ' + %char(sqlerrd(3))")] + [
    "  endif;",
    "  if id = 4;",
    "    exec sql delete from EMPT where current of c1;",
] + ["    " + l for l in P("'dlt ' + " + SC + " + ' ' + %char(sqlerrd(3))")] + [
    "  endif;",
    "enddo;",
    "exec sql close c1;",
    "exec sql update EMPT set SAL = 1 where current of c1;",
] + P("'closed ' + " + SC) + [
    "exec sql open c2;",
    "dow 1 = 1;",
    "  exec sql fetch c2 into :id, :sal;",
    "  if sqlcode <> 0;",
    "    leave;",
    "  endif;",
] + ["  " + l for l in P("%char(id) + ' ' + %char(sal)")] + [
    "enddo;",
    "exec sql close c2;",
] + END

M["SQL007"] = head("EMPT") + [
    "dcl-s id int(10);",
    "dcl-s nm varchar(20);",
    "dcl-s k int(10) inz(-2);",
    "// A scroll cursor in every direction.",
    "exec sql declare c1 scroll cursor for select ID, NAME from EMPT order by ID;",
    "exec sql open c1;",
    "exec sql fetch last from c1 into :id, :nm;",
] + P("'last ' + %char(id) + ' ' + nm + ' ' + " + SC) + [
    "exec sql fetch prior from c1 into :id, :nm;",
] + P("'prior ' + %char(id) + ' ' + nm + ' ' + " + SC) + [
    "exec sql fetch first from c1 into :id, :nm;",
] + P("'first ' + %char(id) + ' ' + nm + ' ' + " + SC) + [
    "exec sql fetch relative 2 from c1 into :id, :nm;",
] + P("'rel 2 ' + %char(id) + ' ' + nm + ' ' + " + SC) + [
    "exec sql fetch relative :k from c1 into :id, :nm;",
] + P("'rel k ' + %char(id) + ' ' + nm + ' ' + " + SC) + [

    "exec sql fetch next from c1 into :id, :nm;",
] + P("'next ' + %char(id) + ' ' + nm + ' ' + " + SC) + [
    "exec sql fetch prior from c1 into :id, :nm;",
] + P("'prior ' + %char(id) + ' ' + nm + ' ' + " + SC) + [
    "exec sql fetch before from c1;",
] + P("'before ' + " + SC) + [
    "exec sql fetch next from c1 into :id, :nm;",
] + P("'next ' + %char(id) + ' ' + nm + ' ' + " + SC) + [
    "exec sql close c1;",
] + END

M["SQL008"] = head("EMPT") + [
    "dcl-ds emp qualified dim(10);",
    "  id int(10);",
    "  nm varchar(20);",
    "  sal packed(9:2);",
    "end-ds;",
    "dcl-s i int(10);",
    "dcl-s n int(10);",
    "dcl-s want int(10) inz(3);",
    "// Multiple-row FETCH into a structure array.",
    "exec sql declare c1 cursor for select ID, NAME, SAL from EMPT order by ID;",
    "exec sql open c1;",
    "exec sql fetch c1 for 3 rows into :emp;",
    "n = sqlerrd(3);",
] + P(SC + " + ' ' + %char(n) + ' ' + %char(sqlerrd(5))") + [
    "for i = 1 to n;",
] + ["  " + l for l in P("%char(emp(i).id) + ' ' + emp(i).nm + ' ' + %char(emp(i).sal)")] + [
    "endfor;",
    "exec sql fetch c1 for :want rows into :emp;",
    "n = sqlerrd(3);",
] + P(SC + " + ' ' + %char(n) + ' ' + %char(sqlerrd(5))") + [
    "for i = 1 to n;",
] + ["  " + l for l in P("%char(emp(i).id) + ' ' + emp(i).nm")] + [
    "endfor;",
    "exec sql fetch c1 for 3 rows into :emp;",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "exec sql close c1;",
] + END

M["SQL009"] = head("EMPT") + [
    "dcl-ds rec qualified;",
    "  id int(10);",
    "  nm varchar(20);",
    "  dp char(3);",
    "  sal packed(9:2);",
    "end-ds;",
    "dcl-s inds int(5) dim(4);",
    "// Host structures, an indicator array, a qualified subfield.",
    "exec sql select ID, NAME, DEPT, SAL into :rec from EMPT where ID = 2;",
] + P(SC + " + ' ' + %char(rec.id) + ' ' + rec.nm") + P("rec.dp + ' ' + %char(rec.sal)") + [
    "exec sql select ID, NAME, DEPT, BONUS into :rec :inds from EMPT where ID = 2;",
] + P(SC + " + ' ' + %char(inds(1)) + ' ' + %char(inds(4))") + P("%char(rec.sal)") + [
    "rec.id = 10;",
    "rec.nm = 'Ivy';",
    "rec.dp = 'OPS';",
    "rec.sal = 1.5;",
    "exec sql insert into EMPT (ID, NAME, DEPT, SAL) values (:rec);",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "exec sql select NAME, SAL into :rec.nm, :rec.sal from EMPT where ID = 10;",
] + P(SC + " + ' ' + rec.nm + ' ' + %char(rec.sal)") + END

M["SQL010"] = head() + [
    "dcl-s n int(10);",
    "dcl-s p packed(7:2);",
    "dcl-s c varchar(30);",
    "dcl-s a char(10) inz('abc');",
    "// VALUES INTO and SET :hv.",
    "exec sql values 6 * 7 into :n;",
] + P(SC + " + ' ' + %char(n)") + [
    "exec sql set :p = 10.5 / 4;",
] + P(SC + " + ' ' + %char(p)") + [
    "exec sql set :c = upper(:a) concat 'X';",
] + P(SC + " + ' ' + c + ' ' + %char(%len(c))") + [
    "exec sql values (length(trim(:a)), substr('Hello', 2, 3)) into :n, :c;",
] + P(SC + " + ' ' + %char(n) + ' ' + c") + [
    "exec sql set :n = coalesce(cast(null as int), 5);",
] + P(SC + " + ' ' + %char(n)") + [
    "exec sql set :p = :p * 2, :n = :n + 1;",
] + P(SC + " + ' ' + %char(p) + ' ' + %char(n)") + END

M["SQL011"] = head("EMPT") + [
    "dcl-s stmt varchar(200);",
    "dcl-s id int(10);",
    "dcl-s nm varchar(20);",
    "dcl-s amt packed(9:2) inz(100);",
    "// PREPARE, EXECUTE USING, EXECUTE IMMEDIATE, a prepared cursor.",
    "stmt = 'UPDATE EMPT SET SAL = SAL + ? WHERE ID = ?';",
    "exec sql prepare s1 from :stmt;",
] + P(SC) + [
    "id = 1;",
    "exec sql execute s1 using :amt, :id;",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "stmt = 'DELETE FROM EMPT WHERE DEPT = ''OPS''';",
    "exec sql execute immediate :stmt;",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "stmt = 'SELECT ID, NAME FROM EMPT WHERE SAL > ? ORDER BY ID';",
    "exec sql prepare s2 from :stmt;",
    "exec sql declare c2 cursor for s2;",
    "amt = 55000;",
    "exec sql open c2 using :amt;",
] + P(SC) + [
    "dow 1 = 1;",
    "  exec sql fetch c2 into :id, :nm;",
    "  if sqlcode <> 0;",
    "    leave;",
    "  endif;",
] + ["  " + l for l in P("%char(id) + ' ' + nm")] + [
    "enddo;",
    "exec sql close c2;",
    "exec sql select SAL into :amt from EMPT where ID = 1;",
] + P(SC + " + ' ' + %char(amt)") + END

M["SQL012"] = head("EMPT") + [
    "dcl-s stmt varchar(200);",
    "// Dynamic statements that fail.",
    "stmt = 'SELEC X FROM EMPT';",
    "exec sql prepare s1 from :stmt;",
] + P(SC) + [
    "stmt = 'SELECT X FROM NOSUCH';",
    "exec sql prepare s1 from :stmt;",
] + P(SC) + [
    "exec sql execute s1;",
] + P(SC) + [
    "stmt = 'DELETE FROM EMPT WHERE ID = 99';",
    "exec sql execute immediate :stmt;",
] + P(SC + " + ' ' + %char(sqlerrd(3))") + [
    "stmt = 'UPDATE EMPT SET NAME = NULL WHERE ID = 1';",
    "exec sql execute immediate :stmt;",
] + P(SC) + [
    "stmt = 'INSERT INTO EMPT (ID, NAME) VALUES (1, ''Dup'')';",
    "exec sql execute immediate :stmt;",
] + P(SC) + END

M["SQL013"] = fhead("EMPT") + [
    D("ID", "S", to="10", typ="I", dec="0"),
    D("NM", "S", to="20", typ="A", kw="VARYING"),
    D("SAL", "S", to="9", typ="P", dec="2"),
    comment("Fixed form: C/EXEC SQL, C+ lines, the short SQLCA names."),
] + sql("SELECT NAME, SAL", "  INTO :NM, :SAL", "  FROM EMPT", "  WHERE ID = 3") + fshow(
    "%CHAR(SQLCOD) + ' ' + SQLSTT + ' ' + NM") + fshow("%CHAR(SAL)") + sql(
    "DECLARE C1 CURSOR FOR", "  SELECT ID FROM EMPT ORDER BY ID DESC") + sql("OPEN C1") + [
    CX("DOU", "SQLCOD <> 0"),
] + sql("FETCH C1 INTO :ID") + [
    CX("IF", "SQLCOD = 0"),
] + fshow("%CHAR(ID) + ' ' + %CHAR(SQLER3)") + [
    C(op="ENDIF"),
    C(op="ENDDO"),
] + fshow("%CHAR(SQLCOD) + ' ' + SQLSTT + ' ' + SQLWN0") + sql("CLOSE C1") + [
    C(op="SETON", hi="LR"),
]

M["SQL014"] = head("EMPT", ctl="ctl-opt dftactgrp(*no) datfmt(*iso) timfmt(*iso);") + [
    "dcl-s d date;",
    "dcl-s t time;",
    "dcl-s ts timestamp;",
    "dcl-s c char(10);",
    "dcl-s n int(10);",
    "dcl-s m int(10);",
    "// Date, time and timestamp host variables.",
    "exec sql select HIRED into :d from EMPT where ID = 3;",
] + P(SC + " + ' ' + %char(d)") + [
    "exec sql select count(*) into :n from EMPT where HIRED > :d;",
] + P(SC + " + ' ' + %char(n)") + [
    "d = d'2022-02-28';",
    "exec sql update EMPT set HIRED = :d where ID = 1;",
    "exec sql select char(HIRED, ISO) into :c from EMPT where ID = 1;",
] + P(SC + " + ' ' + c") + [
    "exec sql values time('13.45.30') into :t;",
] + P(SC + " + ' ' + %char(t)") + [
    "exec sql values timestamp('2026-10-23-13.45.30.000000') into :ts;",
] + P(SC + " + ' ' + %char(ts)") + [
    "exec sql select year(HIRED), month(HIRED) into :n, :m from EMPT where ID = 5;",
] + P(SC + " + ' ' + %char(n) + ' ' + %char(m)") + END

M["SQL015"] = head("EMPT") + [
    "dcl-s z zoned(7:1);",
    "dcl-s i5 int(5);",
    "dcl-s small packed(3:0);",
    "dcl-s c3 char(3);",
    "dcl-s ic int(5);",
    "dcl-s u uns(10);",
    "// Conversions into numeric and character host variables.",
    "exec sql select SAL into :z from EMPT where ID = 2;",
] + P(SC + " + ' ' + %char(z)") + [
    "exec sql select ID * 1000 into :i5 from EMPT where ID = 3;",
] + P(SC + " + ' ' + %char(i5)") + [
    "small = 5;",
    "exec sql select SAL into :small from EMPT where ID = 2;",
] + P(SC + " + ' ' + %char(small)") + [
    "exec sql select SAL into :i5 from EMPT where ID = 2;",
] + P(SC + " + ' ' + %char(i5)") + [
    "exec sql values 'Hello' into :c3 :ic;",
] + P(SC + " + ' ' + c3 + ' ' + %char(ic) + ' ' + sqlwn0 + sqlwn1") + [
    "exec sql values 'Hey' into :c3 :ic;",
] + P(SC + " + ' ' + c3 + ' ' + %char(ic) + ' ' + sqlwn0 + sqlwn1") + [
    "exec sql select SAL into :u from EMPT where ID = 5;",
] + P(SC + " + ' ' + %char(u)") + END

M["SQL016"] = head("EMPT") + [
    "dcl-s n int(10);",
    "dcl-s code int(10);",
    "dcl-s st char(5);",
    "dcl-s nm varchar(20);",
    "// GET DIAGNOSTICS after a change and after no row.",
    "exec sql update EMPT set SAL = SAL + 1 where DEPT = 'DEV';",
    "exec sql get diagnostics :n = ROW_COUNT;",
] + P(SC + " + ' ' + %char(n)") + [
    "exec sql select NAME into :nm from EMPT where ID = 42;",
    "exec sql get diagnostics condition 1 :code = DB2_RETURNED_SQLCODE, :st = RETURNED_SQLSTATE;",
] + P(SC + " + ' ' + %char(code) + ' ' + st") + [
    "exec sql delete from EMPT where ID > 3;",
    "exec sql get diagnostics :n = ROW_COUNT, :code = NUMBER;",
] + P(SC + " + ' ' + %char(n) + ' ' + %char(code)") + END

M["SQL017"] = fhead("EMPT") + [
    D("NM", "S", to="20", typ="A", kw="VARYING"),
    comment("WHENEVER NOT FOUND and SQLERROR with GO TO and CONTINUE."),
] + sql("WHENEVER NOT FOUND GO TO NOTFND") + sql("SELECT NAME INTO :NM FROM EMPT WHERE ID = 77") + fshow(
    "'found'") + [
    C(f1="NOTFND", op="TAG"),
] + fshow("'notfound ' + %CHAR(SQLCOD)") + sql("WHENEVER NOT FOUND CONTINUE") + sql(
    "SELECT NAME INTO :NM FROM EMPT WHERE ID = 78") + fshow("'went on ' + %CHAR(SQLCOD)") + sql(
    "WHENEVER SQLERROR GO TO ERR") + sql("INSERT INTO EMPT (ID, NAME) VALUES (1, 'Dup')") + fshow(
    "'no error'") + [
    C(f1="ERR", op="TAG"),
] + fshow("'error ' + %CHAR(SQLCOD)") + sql("WHENEVER SQLERROR CONTINUE") + sql(
    "INSERT INTO EMPT (ID, NAME) VALUES (1, 'Dup')") + fshow("'again ' + %CHAR(SQLCOD)") + [
    C(op="SETON", hi="LR"),
]

M["SQL018"] = head("EMPT") + [
    "dcl-s c char(10);",
    "dcl-s n int(10);",
    "// SET OPTION first: the date format of character dates.",
    "exec sql set option commit = *none, datfmt = *iso, closqlcsr = *endmod;",
    "exec sql values char(date('2026-01-02')) into :c;",
] + P(SC + " + ' ' + c") + [
    "exec sql select char(HIRED) into :c from EMPT where ID = 2;",
] + P(SC + " + ' ' + c") + [
    "exec sql select count(*) into :n from EMPT where HIRED < '2019-01-01';",
] + P(SC + " + ' ' + %char(n)") + END

M["SQL019"] = head("EMPT") + [
    "dcl-s a int(10);",
    "dcl-s b int(10);",
    "// COMMIT and ROLLBACK without commitment control; WITH HOLD.",
    "exec sql declare c1 cursor with hold for select ID from EMPT order by ID;",
    "exec sql declare c2 cursor for select ID from EMPT order by ID desc;",
    "exec sql open c1;",
    "exec sql open c2;",
    "exec sql fetch c1 into :a;",
    "exec sql fetch c2 into :b;",
] + P(SC + " + ' ' + %char(a) + ' ' + %char(b)") + [
    "exec sql commit;",
] + P("'commit ' + " + SC) + [
    "exec sql fetch c1 into :a;",
] + P("'c1 ' + " + SC + " + ' ' + %char(a)") + [
    "exec sql fetch c2 into :b;",
] + P("'c2 ' + " + SC + " + ' ' + %char(b)") + [
    "exec sql delete from EMPT where ID = 5;",
    "exec sql rollback;",
] + P("'rollback ' + " + SC) + [
    "exec sql select count(*) into :a from EMPT;",
] + P(SC + " + ' ' + %char(a)") + [
    "exec sql fetch c1 into :a;",
] + P("'c1 ' + " + SC + " + ' ' + %char(a)") + [
    "exec sql close c1;",
    "exec sql close c2;",
] + P("'close ' + " + SC) + END

M["SQL020"] = head("EMPT") + [
    "// SQL in a subprocedure, with a parameter as a host variable.",
] + P("getName(1) + ' ' + getName(5) + ' ' + getName(9)") + END + [
    "dcl-proc getName;",
    "  dcl-pi *n varchar(20);",
    "    k int(10) const;",
    "  end-pi;",
    "  dcl-s r varchar(20);",
    "  exec sql select NAME into :r from EMPT where ID = :k;",
    "  if sqlcode <> 0;",
    "    r = '?' + %char(sqlcode);",
    "  endif;",
    "  return r;",
    "end-proc;",
]

M["SQL021"] = head("EMPT") + [
    "dcl-s dp char(3);",
    "dcl-s n int(10);",
    "dcl-s tot packed(11:2);",
    "dcl-s av packed(11:2);",
    "// GROUP BY through a cursor.",
    "exec sql declare c1 cursor for select DEPT, count(*), sum(SAL), avg(SAL) from EMPT",
    "  group by DEPT order by DEPT;",
    "exec sql open c1;",
    "dow 1 = 1;",
    "  exec sql fetch c1 into :dp, :n, :tot, :av;",
    "  if sqlcode <> 0;",
    "    leave;",
    "  endif;",
] + ["  " + l for l in P("dp + ' ' + %char(n) + ' ' + %char(tot) + ' ' + %char(av)")] + [
    "enddo;",
    "exec sql close c1;",
] + END

M["SQL022"] = head("EMPT", extra=["//; run: no"]) + [
    "dcl-s nm varchar(20);",
    "// A host variable that is not defined.",
    "exec sql select NAME into :nosuch from EMPT where ID = 1;",
    "exec sql select NAME into :nm from EMPT where ID = :nokey;",
] + P("nm") + END

M["SQL023"] = head("EMPT", extra=["//; run: no"]) + [
    "dcl-s nm varchar(20);",
    "// Statements that do not parse.",
    "exec sql selec NAME into :nm from EMPT;",
    "exec sql select NAME into :nm from EMPT where;",
    "exec sql select NAME into :nm from EMPT where ID = 1;",
] + P("nm") + END

M["SQL024"] = head() + [
    "dcl-s nm varchar(20);",
    "// A table the precompiler cannot find: created, -204 at run time.",
    "exec sql select NAME into :nm from NOSUCHT where ID = 1;",
] + P(SC) + [
    "exec sql delete from NOSUCHT;",
] + P(SC) + END

M["SQL025"] = head("EMPT", extra=["//; run: no"]) + [
    "dcl-s nm varchar(20);",
    "dcl-s id int(10);",
    "// An RPG error after SQL: the listing numbers generated lines.",
    "exec sql select NAME into :nm from EMPT where ID = 1;",
    "exec sql declare c1 cursor for select ID from EMPT;",
    "exec sql open c1;",
    "exec sql fetch c1 into :id;",
    "exec sql close c1;",
    "nm = undef1;",
    "exec sql update EMPT set SAL = 0 where ID = :id;",
    "id = undef2;",
] + P("nm") + END

M["SQL026"] = head("EMPT", "DEPTT") + [
    "dcl-s pat varchar(10) inz('%e%');",
    "dcl-s nm varchar(20);",
    "dcl-s dn varchar(15);",
    "dcl-s n int(10);",
    "dcl-s lo packed(9:2) inz(50000);",
    "dcl-s hi packed(9:2) inz(60000);",
    "// A join, LIKE, IN and BETWEEN with host variables.",
    "exec sql declare c1 cursor for select E.NAME, D.DNAME from EMPT E join DEPTT D",
    "  on E.DEPT = D.DEPT where E.NAME like :pat order by E.ID;",
    "exec sql open c1;",
    "dow 1 = 1;",
    "  exec sql fetch c1 into :nm, :dn;",
    "  if sqlcode <> 0;",
    "    leave;",
    "  endif;",
] + ["  " + l for l in P("nm + ' ' + dn")] + [
    "enddo;",
    "exec sql close c1;",
    "exec sql select count(*) into :n from EMPT where DEPT in ('ACC', 'OPS');",
] + P(SC + " + ' ' + %char(n)") + [
    "exec sql select count(*) into :n from EMPT where SAL between :lo and :hi;",
] + P(SC + " + ' ' + %char(n)") + [
    "exec sql select DNAME into :dn from DEPTT where DEPT = (select DEPT from EMPT where ID = 4);",
] + P(SC + " + ' ' + dn") + END

M["SQL027"] = head("EMPT") + [
    "dcl-s cf char(20);",
    "dcl-s vv varchar(5);",
    "dcl-s dv varchar(10);",
    "dcl-s n int(10);",
    "dcl-s ind int(5);",
    "// Fixed and varying character host variables.",
    "exec sql select NAME into :cf from EMPT where ID = 1;",
    "line = '<' + cf + '>';",
    "write QSYSPRT line;",
    "exec sql select DEPT into :dv from EMPT where ID = 1;",
] + P(SC + " + ' <' + dv + '> ' + %char(%len(dv))") + [
    "exec sql values 'abcdefgh' into :vv :ind;",
] + P(SC + " + ' ' + vv + ' ' + %char(ind) + ' ' + sqlwn1") + [
    "vv = 'Kim  ';",
    "exec sql insert into EMPT (ID, NAME) values (11, :vv);",
    "exec sql select length(NAME) into :n from EMPT where ID = 11;",
] + P(SC + " + ' ' + %char(n)") + [
    "cf = 'Lou';",
    "exec sql insert into EMPT (ID, NAME) values (12, :cf);",
    "exec sql select length(NAME) into :n from EMPT where ID = 12;",
] + P(SC + " + ' ' + %char(n)") + [
    "exec sql select count(*) into :n from EMPT where NAME = :cf;",
] + P(SC + " + ' ' + %char(n)") + END

M["SQL028"] = head("EMPT", cmd=CMD.replace(" SRCFILE({lib}/QRPGLESRC)", "").replace("COMMIT(*NONE)", "COMMIT(*NONE) OBJTYPE(*MODULE)"),
                   ctl="ctl-opt nomain;", extra=["//; run: no"])[:-2] + [
    "// OBJTYPE(*MODULE): an exported procedure with SQL.",
    "dcl-proc countDept export;",
    "  dcl-pi *n int(10);",
    "    dp char(3) const;",
    "  end-pi;",
    "  dcl-s n int(10);",
    "  exec sql select count(*) into :n from EMPT where DEPT = :dp;",
    "  return n;",
    "end-proc;",
]

M["SQL029"] = head("EMPT") + [
    "dcl-s lim packed(9:2) inz(1);",
    "dcl-s n int(10);",
    "// Input host variables are read when the cursor opens.",
    "exec sql declare c1 cursor for select count(*) from EMPT where SAL > :lim;",
    "lim = 60000;",
    "exec sql open c1;",
    "lim = 0;",
    "exec sql fetch c1 into :n;",
] + P(SC + " + ' ' + %char(n)") + [
    "exec sql close c1;",
    "exec sql open c1;",
    "exec sql fetch c1 into :n;",
] + P(SC + " + ' ' + %char(n)") + [
    "exec sql close c1;",
] + END

M["SQL030"] = head("EMPT") + [
    "dcl-s c varchar(30);",
    "dcl-s w varchar(10);",
    "dcl-s n int(10);",
    "dcl-s b packed(7:2);",
    "dcl-s m packed(11:4);",
    "dcl-s k int(10);",
    "dcl-s c2 varchar(30);",
    "// Expressions and scalar functions in the select list.",
    "exec sql select upper(NAME) concat '-' concat DEPT, length(NAME),",
    "  case when BONUS is null then 'none' else 'some' end, coalesce(BONUS, 0),",
    "  SAL / 12, mod(ID, 2) into :c, :n, :w, :b, :m, :k from EMPT where ID = 2;",
] + P(SC + " + ' ' + c + ' ' + %char(n) + ' ' + w") + P("%char(b) + ' ' + %char(m) + ' ' + %char(k)") + [
    "exec sql select lower(NAME), substr(DEPT, 2), trim(char(ID * 7)) into :c, :w, :c2",
    "  from EMPT where ID = 5;",
] + P(SC + " + ' ' + c + ' ' + w + ' ' + c2") + END

# The lines the precompiler generates: an undefined name after each
# kind of statement shows where the listing numbers it.
M["SQL031"] = head("EMPT", extra=["//; run: no"]) + [
    "dcl-s nm varchar(20);",
    "dcl-s id int(10) inz(2);",
    "dcl-s n int(10);",
    "dcl-ds rec qualified;",
    "  id int(10);",
    "  nm varchar(20);",
    "end-ds;",
    "dcl-ds rows qualified dim(5);",
    "  id int(10);",
    "end-ds;",
    "n = 1;",
    "exec sql select NAME into :nm from EMPT where ID = :id;",
    "n = u1;",
    "exec sql insert into EMPT (ID, NAME) values (:id, :nm);",
    "n = u2;",
    "exec sql select ID, NAME into :rec from EMPT where ID = :id;",
    "n = u3;",
    "exec sql values :id + 1 into :n;",
    "n = u4;",
    "exec sql declare c1 cursor for select ID from EMPT where ID > :id;",
    "n = u5;",
    "exec sql open c1;",
    "n = u6;",
    "exec sql fetch c1 for 2 rows into :rows;",
    "n = u7;",
    "exec sql get diagnostics :n = ROW_COUNT;",
    "n = u8;",
    "exec sql prepare s1 from :nm;",
    "n = u9;",
    "exec sql execute s1 using :id;",
    "n = u10;",
    "exec sql execute immediate :nm;",
    "n = u11;",
    "exec sql commit;",
    "n = u12;",
    "exec sql set :n = 5;",
    "n = u13;",
    "exec sql delete from EMPT where ID = :id;",
    "n = u14;",
    "exec sql close c1;",
    "n = u15;",
    "exec sql update EMPT set NAME = :rec.nm where ID = :rec.id;",
    "n = u16;",
] + END

M["SQL032"] = fhead("EMPT", extra=["//; run: no"]) + [
    D("ID", "S", to="10", typ="I", dec="0"),
    D("NM", "S", to="20", typ="A", kw="VARYING"),
    D("N", "S", to="10", typ="I", dec="0"),
    C(op="Z-ADD", f2="1", res="N"),
] + sql("SELECT NAME", "  INTO :NM", "  FROM EMPT WHERE ID = :ID") + [
    CX("EVAL", "N = U1"),
] + sql("WHENEVER SQLERROR GO TO ERR") + sql("DECLARE C1 CURSOR FOR SELECT ID FROM EMPT") + [
    CX("EVAL", "N = U2"),
] + sql("OPEN C1") + [
    CX("EVAL", "N = U3"),
] + sql("FETCH C1 INTO :ID") + [
    CX("EVAL", "N = U4"),
] + sql("WHENEVER SQLERROR CONTINUE") + sql("CLOSE C1") + [
    CX("EVAL", "N = U5"),
    C(f1="ERR", op="TAG"),
] + sql("UPDATE EMPT SET SAL = 0 WHERE ID = :ID") + [
    CX("EVAL", "N = U6"),
    C(op="SETON", hi="LR"),
]

M["SQL033"] = head("EMPT", extra=["//; run: no"]) + [
    "dcl-s nm varchar(20);",
    "exec sql select NAME into :nm from EMPT where ID = 1;",
    "nm = u1 + nm;",
    "*inlr = *on;",
    "dcl-proc p1;",
    "  dcl-s k int(10);",
    "  dcl-s r varchar(20);",
    "  k = 3;",
    "  exec sql select NAME into :r from EMPT where ID = :k;",
    "  r = u2;",
    "  exec sql update EMPT set NAME = :r where ID = :k;",
    "  r = u3;",
    "end-proc;",
]

os.makedirs(OUT, exist_ok=True)
for name, lines in M.items():
    for i, l in enumerate(lines, 1):
        for piece in l.split("\n"):
            assert len(piece) <= 100, f"{name} line {i} is {len(piece)} wide"
    with open(os.path.join(OUT, name + ".rpgle"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
print(len(M), "members")
