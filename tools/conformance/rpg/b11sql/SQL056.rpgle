**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: DATYP
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-c SQL_NUM 25;
exec sql include SQLDA;
dcl-s txt varchar(300);
// DESCRIBE OUTPUT of every column type: the SQLDA byte for byte, in hex.
txt = 'select * from DATYP';
exec sql prepare s1 from :txt;
SQLN = SQL_NUM;
exec sql describe s1 into :sqlda;
showda();
// The names of expressions and the null-capable entries.
txt = 'select ID + 1, NM concat ''x'' as NX, AMT * 2, count(*), max(D) '
  + 'from DATYP group by ID, NM, AMT';
exec sql prepare s2 from :txt;
SQLN = SQL_NUM;
exec sql describe s2 into :sqlda;
showda();
*inlr = *on;

// The SQLCA's code and the SQLDA: the header, then each entry in two lines.
dcl-proc showda;
  dcl-s i int(10);
  dcl-s n int(10);
  line = %char(sqlcode) + ' ' + sqlstate + ' ' + sqlwarn(1) + sqlwarn(2) + ' '
    + hex(%addr(SQLDA): 16);
  write QSYSPRT line;
  n = SQLD;
  if %subst(SQLDAID: 7: 1) = '2';
    n = SQLD * 2;
  elseif %subst(SQLDAID: 7: 1) = '3';
    n = SQLD * 3;
  endif;
  for i = 1 to n;
    if i > SQLN;
      leave;
    endif;
    line = hex(%addr(SQL_VAR(i)): 48);
    write QSYSPRT line;
    line = '  ' + hex(%addr(SQL_VAR(i)) + 48: 32);
    write QSYSPRT line;
  endfor;
end-proc;

// Bytes as hexadecimal.
dcl-proc hex;
  dcl-pi *n varchar(200);
    p pointer value;
    n int(10) value;
  end-pi;
  dcl-c DIG '0123456789ABCDEF';
  dcl-s c char(1) based(q);
  dcl-s q pointer;
  dcl-ds bb;
    bc char(1);
    bu uns(3) overlay(bc);
  end-ds;
  dcl-s o varchar(200);
  dcl-s i int(10);
  for i = 0 to n - 1;
    q = p + i;
    bc = c;
    o += %subst(DIG: %div(bu: 16) + 1: 1) + %subst(DIG: %rem(bu: 16) + 1: 1);
  endfor;
  return o;
end-proc;
