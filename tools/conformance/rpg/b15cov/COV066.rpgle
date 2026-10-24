**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c char(3);
c = %hival(c);
line = c;
write QSYSPRT line;
*inlr = *on;
