**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds rec;
  dcl-subf read char(3) inz('rd');
  dcl-subf select int(10) inz(4);
  name char(5) inz('nm');
end-ds;
line = %trim(read) + ' ' + %char(select) + ' ' + %trim(name) + ' ' + %char(%size(rec));
write QSYSPRT line;
*inlr = *on;
