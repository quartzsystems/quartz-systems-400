**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s days char(3) dim(7) ctdata perrcd(7);
dcl-s i int(10);
for i = 1 to %elem(days);
  line = %char(i) + ' ' + days(i);
  write QSYSPRT line;
endfor;
*inlr = *on;
**CTDATA days
MONTUEWEDTHUFRISATSUN
