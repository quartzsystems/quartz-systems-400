**FREE
//; run: no
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE)
//; cmd+: OUTPUT(*PRINT) GENLVL(20)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a char(3) inz('abcdef');
line = a;
write QSYSPRT line;
*inlr = *on;
