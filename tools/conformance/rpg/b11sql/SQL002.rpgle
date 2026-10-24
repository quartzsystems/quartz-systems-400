**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s bon packed(7:2);
dcl-s ib int(5);
dcl-s id int(10);
// Null indicators: :v :i, INDICATOR :i, a null without one.
for id = 1 to 5;
  exec sql select BONUS into :bon :ib from EMPT where ID = :id;
  line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(id) + ' ' + %char(bon) + ' ' + %char(ib);
  write QSYSPRT line;
endfor;
bon = 9;
exec sql select BONUS into :bon indicator :ib from EMPT where ID = 2;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(bon) + ' ' + %char(ib);
write QSYSPRT line;
bon = 7;
exec sql select BONUS into :bon from EMPT where ID = 4;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(bon);
write QSYSPRT line;
ib = -1;
exec sql update EMPT set BONUS = :bon :ib where ID = 1;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql select count(*) into :id from EMPT where BONUS is null;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(id);
write QSYSPRT line;
*inlr = *on;
