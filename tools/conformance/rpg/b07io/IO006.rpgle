**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// WRITE new records, then read them all.
CUSNO = 110;
NAME = 'Ivy Irwin';
CITY = 'Austin';
BAL = 10.10;
write CUSTR;
CUSNO = 109;
NAME = 'Jack Jones';
BAL = -9.99;
write CUSTR;
setll *start CUSTF;
read CUSTF;
dow not %eof(CUSTF);
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
read CUSTF;
enddo;
*inlr = *on;
