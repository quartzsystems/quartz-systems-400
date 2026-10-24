**FREE
//; run: no
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT) OPTION(*NOSECLVL *SHOWCPY *EXPDDS *EXT *SHOWSKP *SRCSTMT *DEBUGIO *UNREF)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n int(10) inz(1);
line = %char(n);
write QSYSPRT line;
*inlr = *on;
