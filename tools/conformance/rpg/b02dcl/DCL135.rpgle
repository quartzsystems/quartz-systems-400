**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds mo occurs(3);
  code char(2);
  qty zoned(3:0);
end-ds;
dcl-s i int(10);
for i = 1 to 3;
  %occur(mo) = i;
  code = 'c' + %char(i);
  qty = i * 11;
endfor;
%occur(mo) = 2;
line = code + ' ' + %char(qty) + ' ' + %char(%occur(mo)) + ' ' + %char(%size(mo)) + ' ' +
  %char(%size(mo : *all)) + ' ' + %char(%elem(mo));
write QSYSPRT line;
%occur(mo) = 3;
line = mo;
write QSYSPRT line;
*inlr = *on;
