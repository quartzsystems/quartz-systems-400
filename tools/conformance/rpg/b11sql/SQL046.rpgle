**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-c SQL_NUM 5;
exec sql include SQLDA;
dcl-s txt varchar(200);
dcl-s i int(10);
// DESCRIBE INPUT: the parameter markers of a prepared statement.
txt = 'select NAME from EMPT where ID = ? and DEPT = ?';
exec sql prepare s1 from :txt;
SQLN = SQL_NUM;
exec sql describe input s1 into :sqlda;
line = %char(sqlcode) + ' ' + %char(SQLD);
write QSYSPRT line;
for i = 1 to SQLD;
  SQLVAR = SQL_VAR(i);
  line = %char(SQLTYPE) + ' ' + %char(SQLLEN) + ' ' + %char(SQLNAMELEN) + ' ' + SQLNAME;
  write QSYSPRT line;
endfor;
// Too few entries: SQLD says how many are needed.
SQLN = 0;
exec sql describe s1 into :sqlda;
line = %char(sqlcode) + ' ' + %char(SQLD) + ' ' + %subst(sqlstate: 1: 5);
write QSYSPRT line;
*inlr = *on;
