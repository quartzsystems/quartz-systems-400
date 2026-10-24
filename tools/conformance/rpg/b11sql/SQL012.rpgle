**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s stmt varchar(200);
// Dynamic statements that fail.
stmt = 'SELEC X FROM EMPT';
exec sql prepare s1 from :stmt;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
stmt = 'SELECT X FROM NOSUCH';
exec sql prepare s1 from :stmt;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql execute s1;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
stmt = 'DELETE FROM EMPT WHERE ID = 99';
exec sql execute immediate :stmt;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
stmt = 'UPDATE EMPT SET NAME = NULL WHERE ID = 1';
exec sql execute immediate :stmt;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
stmt = 'INSERT INTO EMPT (ID, NAME) VALUES (1, ''Dup'')';
exec sql execute immediate :stmt;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
*inlr = *on;
