**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds rec qualified;
  id packed(5:0) inz(7);
  nm char(5) inz('abc');
  f ind inz(*on);
end-ds;
dcl-s n packed(7:2) inz(3.5);
dcl-s c varchar(10) inz('hey');
dcl-s arr char(2) dim(3) inz('zz');
line = %char(rec.id) + rec.nm + rec.f + ' ' + %char(n) + c + arr(2);
write QSYSPRT line;
clear rec;
clear n;
clear c;
clear arr;
line = %char(rec.id) + '[' + rec.nm + ']' + rec.f + ' ' + %char(n) + '[' + c + ']' +
      '[' + arr(2) + ']' + %char(%len(c));
write QSYSPRT line;
*inlr = *on;
