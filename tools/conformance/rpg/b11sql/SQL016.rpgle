**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n int(10);
dcl-s code int(10);
dcl-s st char(5);
dcl-s nm varchar(20);
// GET DIAGNOSTICS after a change and after no row.
exec sql update EMPT set SAL = SAL + 1 where DEPT = 'DEV';
exec sql get diagnostics :n = ROW_COUNT;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
exec sql select NAME into :nm from EMPT where ID = 42;
exec sql get diagnostics condition 1 :code = DB2_RETURNED_SQLCODE, :st = RETURNED_SQLSTATE;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(code) + ' ' + st;
write QSYSPRT line;
exec sql delete from EMPT where ID > 3;
exec sql get diagnostics :n = ROW_COUNT, :code = NUMBER;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n) + ' ' + %char(code);
write QSYSPRT line;
*inlr = *on;
