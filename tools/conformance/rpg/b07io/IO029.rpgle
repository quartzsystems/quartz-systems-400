**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; dds: ORDF
//; sqlfile: ORDF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed;
dcl-f ORDF keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s total packed(9:2);
// A join by hand: each customer and the value of its orders.
read CUSTF;
dow not %eof(CUSTF);
total = 0;
setll (CUSNO) ORDF;
reade (CUSNO) ORDF;
dow not %eof(ORDF);
total += QTY * PRICE;
reade (CUSNO) ORDF;
enddo;
line = %char(CUSNO) + ' ' + NAME + %editc(total:'J');
write QSYSPRT line;
read CUSTF;
enddo;
*inlr = *on;
