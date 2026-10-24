**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds item qualified template;
  code char(2);
  qty int(5) dim(3);
end-ds;
dcl-ds order qualified;
  id int(10);
  items likeds(item) dim(2);
end-ds;
dcl-ds orders likeds(order) dim(3);
orders(2).items(2).qty(3) = 77;
orders(3).items(1).code = 'zz';
line = %char(%size(item)) + ' ' + %char(%size(order)) + ' ' + %char(%size(orders)) + ' ' +
  %char(%size(orders : *all));
write QSYSPRT line;
line = %char(%elem(orders)) + ' ' + %char(%elem(order.items)) + ' ' +
  %char(%elem(order.items(1).qty));
write QSYSPRT line;
line = %char(orders(2).items(2).qty(3)) + ' ' + orders(3).items(1).code;
write QSYSPRT line;
line = %char(%addr(orders(2)) - %addr(orders(1))) + ' ' + %char(%addr(orders(1).items(2))
  - %addr(orders(1)));
write QSYSPRT line;
*inlr = *on;
