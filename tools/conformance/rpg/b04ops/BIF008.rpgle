**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s arr packed(5:0) dim(5);
dcl-s c char(7);
dcl-ds ds qualified;
  a char(3);
  b packed(5:2);
end-ds;
dcl-s i int(10);
for i = 1 to %elem(arr);
  arr(i) = i * 10;
endfor;
line = %char(%elem(arr)) + ' ' + %char(%size(arr)) + ' ' + %char(%size(arr:*all));
write QSYSPRT line;
line = %char(%size(c)) + ' ' + %char(%size(ds)) + ' ' + %char(%size(ds.b)) + ' ' +
      %char(%len(ds.b));
write QSYSPRT line;
line = %char(%xfoot(arr));
write QSYSPRT line;
line = %char(%div(17:5)) + ' ' + %char(%rem(17:5)) + ' ' + %char(%div(-17:5)) + ' '
      + %char(%rem(-17:5));
write QSYSPRT line;
*inlr = *on;
