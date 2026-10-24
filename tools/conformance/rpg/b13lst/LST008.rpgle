**FREE
//; run: no
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT) OPTION(*SRCSTMT)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n int(10);
n = 'x';
for n = 1 to 3;
  line = %char(n);
  write QSYSPRT line;
endfor;
*inlr = *on;
