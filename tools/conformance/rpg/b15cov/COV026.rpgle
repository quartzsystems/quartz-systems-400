**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
leavesr;
line = 'x';
write QSYSPRT line;
*inlr = *on;
