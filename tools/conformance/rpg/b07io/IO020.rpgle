**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*input:*output);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// FEOD, and reads after it.
read CUSTF;
read CUSTF;
feod CUSTF;
line = 'feod ' + %char(CUSNO) + ' ' + %error;
write QSYSPRT line;
read CUSTF;
line = 'after ' + %char(CUSNO) + ' ' + %eof(CUSTF);
write QSYSPRT line;
*inlr = *on;
