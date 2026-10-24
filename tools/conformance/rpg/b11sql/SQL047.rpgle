**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-c SQL_NUM 4;
exec sql include SQLDA;
dcl-s txt varchar(200);
dcl-s i int(10);
// PREPARE INTO describes as it prepares; DESCRIBE of a statement
// that is not a query gives no entries.
txt = 'select DEPT, count(*) as N from EMPT group by DEPT';
SQLN = SQL_NUM;
exec sql prepare s1 into :sqlda from :txt;
line = %char(sqlcode) + ' ' + %char(SQLD);
write QSYSPRT line;
for i = 1 to SQLD;
  SQLVAR = SQL_VAR(i);
  line = %char(SQLTYPE) + ' ' + %char(SQLLEN) + ' ' + SQLNAME;
  write QSYSPRT line;
endfor;
txt = 'delete from EMPT where ID = 99';
exec sql prepare s2 from :txt;
exec sql describe s2 into :sqlda;
line = %char(sqlcode) + ' ' + %char(SQLD);
write QSYSPRT line;
exec sql describe s9 into :sqlda;
line = %char(sqlcode) + ' ' + %subst(sqlstate: 1: 5);
write QSYSPRT line;
*inlr = *on;
