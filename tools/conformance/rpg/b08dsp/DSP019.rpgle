**FREE
//; dds: B8R
//; keys: 10,10=1250 enter
//; keys: 10,10=3 enter
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8R workstn;
// Numbers with edit codes and an edit word; a typed number.
N1 = -1234.5;
N2 = 98765.43;
N3 = 0;
N4 = 1234567;
N5 = 102326;
N6 = -1.25;
dou *in03;
exfmt NUMS;
N3 = INN * 2;
N1 = INN - 100;
enddo;
*inlr = *on;
