**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s k packed(5:0) inz(104);
chain k CUSTF;
line = 'found=' + %found(CUSTF) + ' ' + NAME + %char(BAL);
write QSYSPRT line;
chain (999) CUSTR;
line = 'found=' + %found + ' ' + NAME + %char(CUSNO);
write QSYSPRT line;
chain (101) CUSTF;
if %found(CUSTF);
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
endif;
k = 108;
chain (k) CUSTR;
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
line = 'eof=' + %eof(CUSTF);
write QSYSPRT line;
*inlr = *on;
