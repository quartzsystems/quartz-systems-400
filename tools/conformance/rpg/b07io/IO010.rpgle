**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed qualified;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds cus likerec(CUSTF.CUSTR);
dcl-ds cin likerec(CUSTF.CUSTR:*INPUT);
// A qualified file: I/O through data structures.
read CUSTF cus;
line = %char(cus.CUSNO) + ' ' + cus.NAME + cus.CITY;
write QSYSPRT line;
chain (105) CUSTF.CUSTR cin;
line = %char(cin.CUSNO) + ' ' + cin.NAME + %char(cin.BAL);
write QSYSPRT line;
read CUSTF.CUSTR cus;
line = %char(cus.CUSNO) + ' ' + cus.NAME + %char(%size(cus));
write QSYSPRT line;
*inlr = *on;
