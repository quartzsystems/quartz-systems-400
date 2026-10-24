**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s lim packed(9:2) inz(1);
dcl-s n int(10);
// Input host variables are read when the cursor opens.
exec sql declare c1 cursor for select count(*) from EMPT where SAL > :lim;
lim = 60000;
exec sql open c1;
lim = 0;
exec sql fetch c1 into :n;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
exec sql close c1;
exec sql open c1;
exec sql fetch c1 into :n;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
exec sql close c1;
*inlr = *on;
