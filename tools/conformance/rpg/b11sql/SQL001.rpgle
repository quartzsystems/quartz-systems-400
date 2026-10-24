**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s nm varchar(20);
dcl-s dp char(3);
dcl-s sal packed(9:2);
dcl-s id int(10) inz(3);
// SELECT INTO: one row, no row, more than one row, aggregates.
exec sql select NAME, DEPT, SAL into :nm, :dp, :sal from EMPT where ID = :id;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + nm + ' ' + dp + ' ' + %char(sal);
write QSYSPRT line;
id = 99;
exec sql select NAME into :nm from EMPT where ID = :id;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + nm + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql select NAME into :nm from EMPT where DEPT = 'DEV';
line = %char(sqlcode) + ' ' + sqlstate + ' ' + nm + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql select count(*), sum(SAL), max(NAME) into :id, :sal, :nm from EMPT;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(id) + ' ' + %char(sal) + ' ' + nm;
write QSYSPRT line;
*inlr = *on;
