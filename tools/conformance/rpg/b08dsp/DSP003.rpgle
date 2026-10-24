**FREE
//; dds: B8C
//; keys: enter
//; keys: enter
//; keys: 5,10=X enter
//; keys: enter
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8C workstn;
// Option indicators on constants, a field and an attribute.
dou *in03;
COUNT += 1;
*in40 = %rem(COUNT: 2) = 0;
*in41 = COUNT >= 3;
*in42 = COUNT = 2;
*in43 = COUNT >= 2;
*in44 = COUNT >= 4;
STAT = 'Pass ' + %char(COUNT) + ' opt ' + OPT;
exfmt SCR;
enddo;
*inlr = *on;
