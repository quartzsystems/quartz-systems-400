**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; dds: CUSTCITY
ctl-opt dftactgrp(*no);
dcl-f CUSTCITY keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// A logical file keyed by city and number.
read CUSTCITY;
dow not %eof(CUSTCITY);
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
read CUSTCITY;
enddo;
setll ('Chicago') CUSTCITY;
reade ('Chicago') CUSTR;
dow not %eof;
line = 'chi ' + %char(CUSNO) + ' ' + NAME;
write QSYSPRT line;
reade ('Chicago') CUSTR;
enddo;
chain ('Denver':106) CUSTCITY;
line = 'chain ' + %found + ' ' + NAME;
write QSYSPRT line;
*inlr = *on;
