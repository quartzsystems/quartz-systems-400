**FREE
//; with: LSTC3
//; run: no
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/include QRPGLESRC,LSTC3
/copy QRPGLESRC,NOSUCH
line = label;
write QSYSPRT line;
*inlr = *on;
