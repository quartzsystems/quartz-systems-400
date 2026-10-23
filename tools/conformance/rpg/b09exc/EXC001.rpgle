**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s z packed(3:0) inz(0);
dcl-s r packed(5:0);
line = 'before';
write QSYSPRT line;
r = 10 / z;
line = 'after';
write QSYSPRT line;
*inlr = *on;
