**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a int(10) dim(5) ascend;
dcl-s d char(2) dim(4) descend;
a(1) = 9;
a(2) = 3;
a(3) = 7;
a(4) = 1;
a(5) = 5;
d(1) = 'bb';
d(2) = 'dd';
d(3) = 'aa';
d(4) = 'cc';
sorta a;
sorta d;
line = %char(a(1)) + %char(a(2)) + %char(a(3)) + %char(a(4)) + %char(a(5)) + ' ' + d(1) +
  d(2) + d(3) + d(4);
write QSYSPRT line;
line = %char(%lookupge(4 : a)) + ' ' + %char(%lookuple('bc' : d));
write QSYSPRT line;
*inlr = *on;
