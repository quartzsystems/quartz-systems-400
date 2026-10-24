**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s f float(8);
f = %sqrt('16');
line = 'x';
write QSYSPRT line;
*inlr = *on;
