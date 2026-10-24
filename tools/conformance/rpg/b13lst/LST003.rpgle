**FREE
//; with: LSTC2 LSTC3
//; run: no
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/copy LSTC2
line = label + %char(count) + %char(limit);
write QSYSPRT line;
*inlr = *on;
