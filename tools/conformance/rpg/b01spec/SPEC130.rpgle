**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds rec
       qualified
       inz;
  code
    char(3)
    inz('xyz');
  qty packed(5:0)
      inz(12);
  arr
    int(5)
    dim(3);
end-ds;
line = rec.code + ' ' + %char(rec.qty) + ' ' + %char(rec.arr(2)) + ' ' + %char(%size(rec));
write QSYSPRT line;
*inlr = *on;
