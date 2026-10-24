**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-c SQL_NUM 4;
exec sql include SQLDA;
dcl-s txt varchar(200);
dcl-s id int(10);
dcl-s nm char(20);
dcl-s dp char(3);
dcl-s dpi int(5);
// FETCH USING DESCRIPTOR: the entries point at the variables.
txt = 'select ID, NAME, DEPT from EMPT where ID < 5 order by ID';
exec sql prepare s1 from :txt;
exec sql declare c1 cursor for s1;
SQLN = SQL_NUM;
SQLD = 3;
SQLVAR = SQL_VAR(1);
SQLTYPE = 496;
SQLLEN = 4;
SQLDATA = %addr(id);
SQL_VAR(1) = SQLVAR;
SQLVAR = SQL_VAR(2);
SQLTYPE = 452;
SQLLEN = 20;
SQLDATA = %addr(nm);
SQL_VAR(2) = SQLVAR;
SQLVAR = SQL_VAR(3);
SQLTYPE = 453;
SQLLEN = 3;
SQLDATA = %addr(dp);
SQLIND = %addr(dpi);
SQL_VAR(3) = SQLVAR;
exec sql open c1;
dow 1 = 1;
  exec sql fetch c1 using descriptor :sqlda;
  if sqlcode <> 0;
    leave;
  endif;
  line = %char(id) + ' ' + nm + ' ' + dp + ' ' + %char(dpi);
  write QSYSPRT line;
enddo;
line = 'end ' + %char(sqlcode);
write QSYSPRT line;
exec sql close c1;
*inlr = *on;
