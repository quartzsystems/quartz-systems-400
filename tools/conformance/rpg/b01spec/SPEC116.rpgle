**FREE
//; with: SPC116C
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/copy SPC116C
line = fxa + ' ' + %char(fxn);
write QSYSPRT line;
*inlr = *on;
