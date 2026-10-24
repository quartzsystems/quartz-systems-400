**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s z zoned(7:1);
dcl-s i5 int(5);
dcl-s small packed(3:0);
dcl-s c3 char(3);
dcl-s ic int(5);
dcl-s u uns(10);
// Conversions into numeric and character host variables.
exec sql select SAL into :z from EMPT where ID = 2;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(z);
write QSYSPRT line;
exec sql select ID * 1000 into :i5 from EMPT where ID = 3;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(i5);
write QSYSPRT line;
small = 5;
exec sql select SAL into :small from EMPT where ID = 2;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(small);
write QSYSPRT line;
exec sql select SAL into :i5 from EMPT where ID = 2;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(i5);
write QSYSPRT line;
exec sql values 'Hello' into :c3 :ic;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + c3 + ' ' + %char(ic) + ' ' + sqlwn0 + sqlwn1;
write QSYSPRT line;
exec sql values 'Hey' into :c3 :ic;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + c3 + ' ' + %char(ic) + ' ' + sqlwn0 + sqlwn1;
write QSYSPRT line;
exec sql select SAL into :u from EMPT where ID = 5;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(u);
write QSYSPRT line;
*inlr = *on;
