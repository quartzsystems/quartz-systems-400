**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds rec qualified;
  id int(10);
  nm varchar(20);
  dp char(3);
  sal packed(9:2);
end-ds;
dcl-s inds int(5) dim(4);
// Host structures, an indicator array, a qualified subfield.
exec sql select ID, NAME, DEPT, SAL into :rec from EMPT where ID = 2;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(rec.id) + ' ' + rec.nm;
write QSYSPRT line;
line = rec.dp + ' ' + %char(rec.sal);
write QSYSPRT line;
exec sql select ID, NAME, DEPT, BONUS into :rec :inds from EMPT where ID = 2;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(inds(1)) + ' ' + %char(inds(4));
write QSYSPRT line;
line = %char(rec.sal);
write QSYSPRT line;
rec.id = 10;
rec.nm = 'Ivy';
rec.dp = 'OPS';
rec.sal = 1.5;
exec sql insert into EMPT (ID, NAME, DEPT, SAL) values (:rec);
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql select NAME, SAL into :rec.nm, :rec.sal from EMPT where ID = 10;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + rec.nm + ' ' + %char(rec.sal);
write QSYSPRT line;
*inlr = *on;
