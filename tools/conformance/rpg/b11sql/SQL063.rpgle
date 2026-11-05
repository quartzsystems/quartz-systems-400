**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds emp qualified dim(3);
  id int(10);
  name varchar(20);
  dept char(3);
end-ds;
dcl-s ind int(5) dim(3);
dcl-ds inds qualified dim(3);
  i int(5) dim(3);
end-ds;
dcl-s n int(10);
dcl-s ix int(5);
dcl-s rc int(10);
dcl-s st char(5);
dcl-s mid char(7);
dcl-s mt varchar(200);
dcl-s cnt int(10);
dcl-s more char(1);
dcl-s cmd varchar(30);
dcl-s txt varchar(100);
// DECLARE STATEMENT and TABLE, blocked INSERT, indicator arrays, GET DIAGNOSTICS.
exec sql declare s1 statement;
exec sql declare EMPT table (ID int not null, NAME varchar(20) not null,
  DEPT char(3), SAL decimal(9, 2), HIRED date, BONUS decimal(7, 2));
emp(1).id = 11;
emp(1).name = 'Kim';
emp(1).dept = 'ACC';
emp(2).id = 12;
emp(2).name = 'Lou';
emp(2).dept = 'DEV';
inds(1).i(3) = 0;
inds(2).i(3) = -1;
exec sql insert into EMPT (ID, NAME, DEPT) 2 rows values (:emp :inds);
line = 'insert ' + %char(sqlcode) + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql get diagnostics :rc = ROW_COUNT, :cnt = NUMBER, :more = MORE, :cmd = COMMAND_FUNCTION;
line = 'diag ' + %char(sqlcode) + ' ' + %char(rc) + ' ' + %char(cnt) + ' ' + more + ' ' + cmd;
write QSYSPRT line;
exec sql select count(*) into :n from EMPT where DEPT is null;
line = 'nulls ' + %char(sqlcode) + ' ' + %char(n);
write QSYSPRT line;
exec sql declare c1 cursor for select ID, NAME, DEPT from EMPT where ID > 3 order by ID;
exec sql open c1;
exec sql fetch c1 for 3 rows into :emp :inds;
line = 'fetch ' + %char(sqlcode) + ' ' + %char(sqlerrd(3)) + ' ' + %char(sqlerrd(5)) + ' '
  + %char(emp(3).id) + ' ' + %char(inds(3).i(3));
write QSYSPRT line;
exec sql close c1;
exec sql select BONUS into :n :ix from EMPT where ID = 2;
line = 'ind ' + %char(sqlcode) + ' ' + %char(ix);
write QSYSPRT line;
exec sql select NAME into :txt from NOSUCH where ID = 1;
exec sql get diagnostics condition 1 :rc = DB2_RETURNED_SQLCODE, :st = RETURNED_SQLSTATE,
  :mid = DB2_MESSAGE_ID, :mt = MESSAGE_TEXT;
line = 'cond ' + %char(sqlcode) + ' ' + %char(rc) + ' ' + st + ' ' + mid;
write QSYSPRT line;
exec sql delete from EMPT where ID > 10;
*inlr = *on;
