**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s g graph(5);
g = %graph(12);
line = 'x';
write QSYSPRT line;
*inlr = *on;
