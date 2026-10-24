**FREE
//; dds: B8P
ctl-opt dftactgrp(*no);
dcl-f B8P printer oflind(*in90);
dcl-s i int(10);
// An externally described printer file: formats, an option indicator.
write HDR;
for i = 1 to 8;
DNUM = i;
DNAME = 'Name ' + %char(i);
DAMT = i * 12.5;
*in50 = %rem(i: 3) = 0;
TAMT += DAMT;
write DTL;
if *in90;
write HDR;
*in90 = *off;
endif;
endfor;
write TOT;
*inlr = *on;
