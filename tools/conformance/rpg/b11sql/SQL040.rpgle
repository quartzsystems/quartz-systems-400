**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n int(10);
// CLOSQLCSR = *ENDSQL is for programs that are not ILE.
exec sql set option closqlcsr = *endsql;
exec sql select count(*) into :n from EMPT;
line = %char(sqlcode) + ' ' + %char(n);
write QSYSPRT line;
*inlr = *on;
