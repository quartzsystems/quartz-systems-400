**free
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
line = 'lower-case free marker';
write QSYSPRT line;
*inlr = *on;
