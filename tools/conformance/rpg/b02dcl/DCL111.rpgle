**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds a inz;
  a1 char(3);
  a2 packed(5:2) inz(1.5);
  a3 zoned(3:0);
  a4 int(5);
  a5 ind;
  a6 varchar(5);
  a7 date;
end-ds;
dcl-ds b;
  b1 char(3) inz('bb');
  b2 zoned(3:0) inz;
  b3 char(2);
end-ds;
line = '[' + a1 + '] ' + %char(a2) + ' ' + %char(a3) + ' ' + %char(a4) + ' ' + a5 + ' ' +
  %char(%len(a6)) + ' ' + %char(a7);
write QSYSPRT line;
line = '[' + b + '] ' + %char(b2);
write QSYSPRT line;
*inlr = *on;
