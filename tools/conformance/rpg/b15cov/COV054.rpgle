**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
line = %proc(1);
write QSYSPRT line;
*inlr = *on;
