**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s stmt varchar(200);
dcl-s id int(10);
dcl-s nm varchar(20);
dcl-s amt packed(9:2) inz(100);
// PREPARE, EXECUTE USING, EXECUTE IMMEDIATE, a prepared cursor.
stmt = 'UPDATE EMPT SET SAL = SAL + ? WHERE ID = ?';
exec sql prepare s1 from :stmt;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
id = 1;
exec sql execute s1 using :amt, :id;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
stmt = 'DELETE FROM EMPT WHERE DEPT = ''OPS''';
exec sql execute immediate :stmt;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
stmt = 'SELECT ID, NAME FROM EMPT WHERE SAL > ? ORDER BY ID';
exec sql prepare s2 from :stmt;
exec sql declare c2 cursor for s2;
amt = 55000;
exec sql open c2 using :amt;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
dow 1 = 1;
  exec sql fetch c2 into :id, :nm;
  if sqlcode <> 0;
    leave;
  endif;
  line = %char(id) + ' ' + nm;
  write QSYSPRT line;
enddo;
exec sql close c2;
exec sql select SAL into :amt from EMPT where ID = 1;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(amt);
write QSYSPRT line;
*inlr = *on;
