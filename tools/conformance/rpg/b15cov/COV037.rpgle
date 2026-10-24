**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds item qualified template;
  id int(10);
end-ds;
dcl-s total int(10);
xml-into %handler(got:total) %xml('<l><item><id>1</id></item><item><id>2</id></item>' +
  '<item><id>3</id></item></l>':'doc=string path=l/item');
line = 'total ' + %char(total);
write QSYSPRT line;
*inlr = *on;
dcl-proc got;
  dcl-pi *n int(10);
    sum int(10);
    items likeds(item) dim(2) const;
    n uns(10) value;
  end-pi;
  dcl-s i int(10);
  for i = 1 to n;
    sum += items(i).id;
  endfor;
  line = 'got ' + %char(n);
  write QSYSPRT line;
  return 0;
end-proc;
