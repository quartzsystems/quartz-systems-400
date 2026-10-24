**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*CHG) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no) actgrp(*new);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n int(10);
// COMMIT(*CHG) over a table that is not journaled: changes fail, queries
// work, COMMIT and ROLLBACK have nothing to do.
exec sql update EMPT set SAL = 1 where ID = 1;
line = 'update ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql insert into EMPT (ID, NAME) values (9, 'Zed');
line = 'insert ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql select count(*) into :n from EMPT;
line = 'select ' + %char(sqlcode) + ' ' + %char(n);
write QSYSPRT line;
exec sql commit;
line = 'commit ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql rollback;
line = 'rollback ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
*inlr = *on;
