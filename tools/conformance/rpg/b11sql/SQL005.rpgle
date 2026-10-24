**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s pct packed(3:2) inz(1.10);
dcl-s dp char(3) inz('DEV');
dcl-s sal packed(11:2);
dcl-s n int(10);
// Searched UPDATE and DELETE; no row is +100.
exec sql update EMPT set SAL = SAL * :pct where DEPT = :dp;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql update EMPT set SAL = 0 where DEPT = 'XXX';
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql select sum(SAL) into :sal from EMPT where DEPT = :dp;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sal);
write QSYSPRT line;
exec sql delete from EMPT where DEPT = 'ACC';
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql delete from EMPT where DEPT = 'ACC';
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql select count(*) into :n from EMPT;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
*inlr = *on;
