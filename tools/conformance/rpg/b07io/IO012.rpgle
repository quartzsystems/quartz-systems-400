**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usropn;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// USROPN: OPEN, CLOSE and %OPEN.
line = 'open=' + %open(CUSTF);
write QSYSPRT line;
open CUSTF;
line = 'open=' + %open(CUSTF);
write QSYSPRT line;
read CUSTF;
read CUSTF;
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
close CUSTF;
line = 'open=' + %open(CUSTF);
write QSYSPRT line;
open CUSTF;
read CUSTF;
line = 'again ' + %char(CUSNO);
write QSYSPRT line;
close *all;
open QSYSPRT;
line = 'open=' + %open(CUSTF);
write QSYSPRT line;
*inlr = *on;
