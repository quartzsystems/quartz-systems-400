**FREE
//; dds: B8H
//; keys: 4,10=AAA enter
//; keys: f6
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8H workstn;
dcl-s n int(10);
// INDARA without a structure: the area is *IN01-*IN99.
dou *in03;
n += 1;
*in30 = n = 2;
*in31 = n = 3;
RES = 'n=' + %char(n) + ' in06=' + *in06 + ' inp=' + INP;
exfmt SCR;
enddo;
*inlr = *on;
