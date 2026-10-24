**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; with: SQL042I
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// INCLUDE of a member of the source file: its declarations.
exec sql include SQL042I;
exec sql select count(*), max(NAME) into :cnt, :nm from EMPT;
line = %char(sqlcode) + ' ' + %char(cnt) + ' ' + nm;
write QSYSPRT line;
*inlr = *on;
