**FREE
//; with: SPC100I
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/include SPC100I
line = incfld + ' ' + %char(INCK);
write QSYSPRT line;
*inlr = *on;
