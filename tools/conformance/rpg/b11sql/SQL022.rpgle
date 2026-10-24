**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; run: no
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s nm varchar(20);
// A host variable that is not defined.
exec sql select NAME into :nosuch from EMPT where ID = 1;
exec sql select NAME into :nm from EMPT where ID = :nokey;
line = nm;
write QSYSPRT line;
*inlr = *on;
