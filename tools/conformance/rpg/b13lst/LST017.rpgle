**FREE
//; run: no
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE)
//; cmd+: OUTPUT(*PRINT) INDENT('>')
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s i int(10);
dcl-s j int(10);
for i = 1 to 3;
  if i = 2;
    j += 10;
  else;
    j += 1;
  endif;
endfor;
line = %char(j);
write QSYSPRT line;
*inlr = *on;
