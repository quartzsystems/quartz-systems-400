**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; run: no
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s nm varchar(20);
dcl-s id int(10);
// An RPG error after SQL: the listing numbers generated lines.
exec sql select NAME into :nm from EMPT where ID = 1;
exec sql declare c1 cursor for select ID from EMPT;
exec sql open c1;
exec sql fetch c1 into :id;
exec sql close c1;
nm = undef1;
exec sql update EMPT set SAL = 0 where ID = :id;
id = undef2;
line = nm;
write QSYSPRT line;
*inlr = *on;
