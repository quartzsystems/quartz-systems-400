**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; run: no
ctl-opt dftactgrp(*no);
dcl-s n int(10);
// INCLUDE of a member that is not there.
exec sql include NOSUCHMBR;
exec sql values 1 into :n;
*inlr = *on;
