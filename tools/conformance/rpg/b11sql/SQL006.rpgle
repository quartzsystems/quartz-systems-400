**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s id int(10);
dcl-s sal packed(9:2);
// Positioned UPDATE and DELETE through a FOR UPDATE cursor.
exec sql declare c1 cursor for select ID, SAL from EMPT for update of SAL;
exec sql declare c2 cursor for select ID, SAL from EMPT order by ID;
exec sql open c1;
dow 1 = 1;
  exec sql fetch c1 into :id, :sal;
  if sqlcode <> 0;
    leave;
  endif;
  if id = 2;
    exec sql update EMPT set SAL = SAL + 1000 where current of c1;
    line = 'upd ' + %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
    write QSYSPRT line;
  endif;
  if id = 4;
    exec sql delete from EMPT where current of c1;
    line = 'dlt ' + %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
    write QSYSPRT line;
  endif;
enddo;
exec sql close c1;
exec sql update EMPT set SAL = 1 where current of c1;
line = 'closed ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql open c2;
dow 1 = 1;
  exec sql fetch c2 into :id, :sal;
  if sqlcode <> 0;
    leave;
  endif;
  line = %char(id) + ' ' + %char(sal);
  write QSYSPRT line;
enddo;
exec sql close c2;
*inlr = *on;
