**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a int(10) dim(*auto : 10);
dcl-s v char(3) dim(*var : 5);
dcl-s i int(10);
line = %char(%elem(a)) + ' ' + %char(%elem(a : *max)) + ' ' + %char(%elem(v));
write QSYSPRT line;
a(*next) = 5;
a(*next) = 7;
a(4) = 9;
%elem(v) = 2;
v(1) = 'aa';
v(2) = 'bb';
line = %char(%elem(a)) + ' ' + %char(a(1)) + %char(a(2)) + %char(a(3)) + %char(a(4)) + ' '
  + %char(%elem(v)) + v(1) + v(2);
write QSYSPRT line;
%elem(a) = 1;
%elem(v) = 4;
line = %char(%elem(a)) + ' ' + %char(%elem(v)) + '[' + v(4) + ']';
write QSYSPRT line;
*inlr = *on;
