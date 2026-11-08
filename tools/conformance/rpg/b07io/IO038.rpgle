**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// CLEAR and CLEAR *ALL of the format of a file the program has read only.
read CUSTF;
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
clear CUSTR;
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
clear *all CUSTR;
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
*inlr = *on;
