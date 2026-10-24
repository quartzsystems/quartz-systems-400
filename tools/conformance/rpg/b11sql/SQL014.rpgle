**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no) datfmt(*iso) timfmt(*iso);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s d date;
dcl-s t time;
dcl-s ts timestamp;
dcl-s c char(10);
dcl-s n int(10);
dcl-s m int(10);
// Date, time and timestamp host variables.
exec sql select HIRED into :d from EMPT where ID = 3;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(d);
write QSYSPRT line;
exec sql select count(*) into :n from EMPT where HIRED > :d;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
d = d'2022-02-28';
exec sql update EMPT set HIRED = :d where ID = 1;
exec sql select char(HIRED, ISO) into :c from EMPT where ID = 1;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + c;
write QSYSPRT line;
exec sql values time('13.45.30') into :t;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(t);
write QSYSPRT line;
exec sql values timestamp('2026-10-23-13.45.30.000000') into :ts;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(ts);
write QSYSPRT line;
exec sql select year(HIRED), month(HIRED) into :n, :m from EMPT where ID = 5;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n) + ' ' + %char(m);
write QSYSPRT line;
*inlr = *on;
