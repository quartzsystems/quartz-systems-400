**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f CUSTF disk usage(*input) rafdata(CUSTF);
read CUSTF;
line = 'x';
write QSYSPRT line;
*inlr = *on;
