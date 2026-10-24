**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s x int(10);
line = %char(%parmnum(x));
write QSYSPRT line;
*inlr = *on;
