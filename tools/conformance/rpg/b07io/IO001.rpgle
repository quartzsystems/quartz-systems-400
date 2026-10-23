**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// Sequential reads by file and by format; %EOF both ways.
read CUSTF;
dow not %eof(CUSTF);
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
read CUSTR;
enddo;
line = 'eof=' + %eof + ' ' + %eof(CUSTF);
write QSYSPRT line;
*inlr = *on;
