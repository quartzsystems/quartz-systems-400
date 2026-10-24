**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a int(10);
dcl-s b int(10);
// COMMIT and ROLLBACK without commitment control; WITH HOLD.
exec sql declare c1 cursor with hold for select ID from EMPT order by ID;
exec sql declare c2 cursor for select ID from EMPT order by ID desc;
exec sql open c1;
exec sql open c2;
exec sql fetch c1 into :a;
exec sql fetch c2 into :b;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(a) + ' ' + %char(b);
write QSYSPRT line;
exec sql commit;
line = 'commit ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql fetch c1 into :a;
line = 'c1 ' + %char(sqlcode) + ' ' + sqlstate + ' ' + %char(a);
write QSYSPRT line;
exec sql fetch c2 into :b;
line = 'c2 ' + %char(sqlcode) + ' ' + sqlstate + ' ' + %char(b);
write QSYSPRT line;
exec sql delete from EMPT where ID = 5;
exec sql rollback;
line = 'rollback ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql select count(*) into :a from EMPT;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(a);
write QSYSPRT line;
exec sql fetch c1 into :a;
line = 'c1 ' + %char(sqlcode) + ' ' + sqlstate + ' ' + %char(a);
write QSYSPRT line;
exec sql close c1;
exec sql close c2;
line = 'close ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
*inlr = *on;
