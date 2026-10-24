**FREE
//; dds: B8N
//; keys: pgdn
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8N workstn sfile(SFL4: rrn);
dcl-s rrn packed(4: 0);
dcl-s i int(10);
// SFLRCDNBR: the page with record 7 shows first.
*in33 = *on;
write CTL4;
*in33 = *off;
for i = 1 to 12;
NNUM = i;
rrn = i;
write SFL4;
endfor;
TOREC = 7;
dou *in03;
exfmt CTL4;
enddo;
*inlr = *on;
