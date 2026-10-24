**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-c SQL_NUM 10;
exec sql include SQLDA;
dcl-s txt varchar(200);
dcl-s i int(10);
// DESCRIBE of a prepared SELECT: the SQLDA's header and entries.
txt = 'select ID, NAME, DEPT, SAL, HIRED, BONUS, ID * 2 as TWICE from EMPT';
exec sql prepare s1 from :txt;
SQLN = SQL_NUM;
exec sql describe s1 into :sqlda;
line = %char(sqlcode) + ' ' + %trim(SQLDAID) + ' ' + %char(SQLDABC) + ' '
  + %char(SQLN) + ' ' + %char(SQLD);
write QSYSPRT line;
for i = 1 to SQLD;
  SQLVAR = SQL_VAR(i);
  line = %char(SQLTYPE) + ' ' + %char(SQLLEN) + ' ' + %char(SQLNAMELEN) + ' ' + SQLNAME;
  write QSYSPRT line;
endfor;
*inlr = *on;
