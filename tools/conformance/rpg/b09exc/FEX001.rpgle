**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// A duplicate key with no (E), indicator or INFSR: the default handler.
line = 'before';
write QSYSPRT line;
CUSNO = 101;
write CUSTR;
line = 'after';
write QSYSPRT line;
*inlr = *on;
