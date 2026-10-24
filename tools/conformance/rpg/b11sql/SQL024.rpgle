**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s nm varchar(20);
// A table the precompiler cannot find: created, -204 at run time.
exec sql select NAME into :nm from NOSUCHT where ID = 1;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql delete from NOSUCHT;
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
*inlr = *on;
