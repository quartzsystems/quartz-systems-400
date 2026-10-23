**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds rec likerec(CUSTR:*ALL);
dcl-ds inp likerec(CUSTR:*INPUT);
// Data structure I/O: read into a structure, write and update from one.
read CUSTF inp;
line = 'ds ' + %char(inp.CUSNO) + ' ' + inp.NAME + ' fld ' + %char(CUSNO) + '|';
write QSYSPRT line;
rec.CUSNO = 120;
rec.NAME = 'From DS';
rec.CITY = 'Austin';
rec.BAL = 1.5;
write CUSTR rec;
chain (120) CUSTF rec;
rec.BAL = 2.5;
update CUSTR rec;
chain(n) (120) CUSTF;
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
*inlr = *on;
