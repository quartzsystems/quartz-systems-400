**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c char(10);
dcl-s n int(10);
// SET OPTION first: the date format of character dates.
exec sql set option commit = *none, datfmt = *iso, closqlcsr = *endmod;
exec sql values char(date('2026-01-02')) into :c;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + c;
write QSYSPRT line;
exec sql select char(HIRED) into :c from EMPT where ID = 2;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + c;
write QSYSPRT line;
exec sql select count(*) into :n from EMPT where HIRED < '2019-01-01';
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
*inlr = *on;
