**FREE
//; with: LSTC4
//; run: no
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/copy LSTC4
dcl-s after char(2) inz('xyz');
line = %char(inside) + after;
write QSYSPRT line;
*inlr = *on;
