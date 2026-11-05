**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s id int(10);
// Cursor declarations the precompiler or the run refuses.
exec sql declare c1 cursor for select ID from EMPT order by ID;
exec sql declare c2 insensitive scroll cursor for select ID from EMPT for update of NAME;
exec sql declare c3 sensitive dynamic scroll cursor for select ID from EMPT;
exec sql open c1;
exec sql fetch prior from c1 into :id;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql open c2;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql open c3;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
*inlr = *on;
