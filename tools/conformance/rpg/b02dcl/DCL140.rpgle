**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds a len(20);
  a1 char(3);
end-ds;
dcl-ds b;
  b1 char(3) pos(10);
  b2 char(2);
end-ds;
dcl-ds c len(5) end-ds;
c = 'abcdefg';
line = %char(%size(a)) + ' ' + %char(%size(b)) + ' ' + %char(%addr(b2) - %addr(b)) + ' ' +
  c;
write QSYSPRT line;
*inlr = *on;
