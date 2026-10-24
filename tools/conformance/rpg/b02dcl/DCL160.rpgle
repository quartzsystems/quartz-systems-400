**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds e qualified dim(4);
  k int(5);
  v char(3);
end-ds;
dcl-s i int(10);
for i = 1 to %elem(e);
  e(i).k = i * i;
  e(i).v = 'v' + %char(i);
endfor;
sorta(d) e(*).k;
line = %char(%size(e)) + ' ' + %char(%size(e : *all)) + ' ' + %char(%addr(e(3)) -
  %addr(e(1)));
write QSYSPRT line;
line = %char(e(4).k) + e(4).v + ' ' + %char(%lookup(9 : e(*).k));
write QSYSPRT line;
*inlr = *on;
