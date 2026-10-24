**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds a;
  x char(4) pos(9);
  y char(2);
  z char(3) pos(2);
  w char(1);
end-ds;
line = %char(%size(a)) + ' ' + %char(%addr(y) - %addr(a)) + ' ' + %char(%addr(w) -
  %addr(a));
write QSYSPRT line;
*inlr = *on;
