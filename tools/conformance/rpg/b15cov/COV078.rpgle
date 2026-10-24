**FREE
//; dds: TYPESF
//; sqlfile: TYPESF
ctl-opt dftactgrp(*no) extbinint(*yes);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds rec extname('TYPESF') qualified end-ds;
rec.tbin = 2000000000;
line = 'size ' + %char(%size(rec.tbin)) + ' ' + %char(rec.tbin) + ' ' + %char(%size(rec.tbin2));
write QSYSPRT line;
*inlr = *on;
