**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds emp qualified dim(10);
  id int(10);
  nm varchar(20);
  sal packed(9:2);
end-ds;
dcl-s i int(10);
dcl-s n int(10);
dcl-s want int(10) inz(3);
// Multiple-row FETCH into a structure array.
exec sql declare c1 cursor for select ID, NAME, SAL from EMPT order by ID;
exec sql open c1;
exec sql fetch c1 for 3 rows into :emp;
n = sqlerrd(3);
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n) + ' ' + %char(sqlerrd(5));
write QSYSPRT line;
for i = 1 to n;
  line = %char(emp(i).id) + ' ' + emp(i).nm + ' ' + %char(emp(i).sal);
  write QSYSPRT line;
endfor;
exec sql fetch c1 for :want rows into :emp;
n = sqlerrd(3);
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n) + ' ' + %char(sqlerrd(5));
write QSYSPRT line;
for i = 1 to n;
  line = %char(emp(i).id) + ' ' + emp(i).nm;
  write QSYSPRT line;
endfor;
exec sql fetch c1 for 3 rows into :emp;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql close c1;
*inlr = *on;
