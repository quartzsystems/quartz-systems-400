**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*delete);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// DELETE after a read and by key; %FOUND.
chain (103) CUSTF;
delete CUSTR;
delete (106) CUSTF;
line = 'del 106 found=' + %found(CUSTF);
write QSYSPRT line;
delete (999) CUSTF;
line = 'del 999 found=' + %found(CUSTF);
write QSYSPRT line;
setll *start CUSTF;
read(n) CUSTF;
dow not %eof(CUSTF);
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
read(n) CUSTF;
enddo;
*inlr = *on;
