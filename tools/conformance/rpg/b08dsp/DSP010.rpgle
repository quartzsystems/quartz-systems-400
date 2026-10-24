**FREE
//; dds: B8J
//; keys: pgdn
//; keys: pgdn
//; keys: pgup
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8J workstn sfile(SFL1: rrn);
dcl-s rrn packed(4: 0);
dcl-s i int(10);
// A subfile loaded whole; the system rolls its pages.
*in33 = *on;
write CTL1;
*in33 = *off;
for i = 1 to 10;
SNUM = i * 3;
SNAM = 'Entry number ' + %char(i);
rrn = i;
write SFL1;
endfor;
*in31 = *on;
*in32 = *on;
*in34 = *on;
dou *in03;
write FTR;
exfmt CTL1;
enddo;
*inlr = *on;
