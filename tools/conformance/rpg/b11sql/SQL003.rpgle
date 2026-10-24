**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s id int(10);
dcl-s nm varchar(20);
// A cursor: fetch before open, open twice, the loop, close twice.
exec sql declare c1 cursor for select ID, NAME from EMPT where SAL > 50000 order by SAL desc;
exec sql fetch c1 into :id, :nm;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql open c1;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql open c1;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
dow 1 = 1;
  exec sql fetch c1 into :id, :nm;
  if sqlcode <> 0;
    leave;
  endif;
  line = %char(id) + ' ' + nm + ' ' + %char(sqlerrd(3));
  write QSYSPRT line;
enddo;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql close c1;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql close c1;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
*inlr = *on;
