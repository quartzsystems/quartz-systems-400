**FREE
//; dds: COVNUL
//; sql: INSERT INTO {lib}.COVNUL VALUES (1, 'one'), (2, NULL), (3, 'three')
ctl-opt dftactgrp(*no) alwnull(*inputonly);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f COVNUL disk usage(*input);
read COVNUL;
read COVNUL;
line = %char(NKEY) + ' [' + NVAL + ']';
write QSYSPRT line;
*inlr = *on;
