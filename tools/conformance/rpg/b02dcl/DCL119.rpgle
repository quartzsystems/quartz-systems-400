**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s code char(1) dim(4) ctdata perrcd(2) ascend;
dcl-s amt packed(5:2) dim(4) alt(code);
dcl-s i int(10);
for i = 1 to 4;
line = code(i) + ' ' + %char(amt(i));
  write QSYSPRT line;
endfor;
line = %char(%lookup('C' : code)) + ' ' + %char(%lookupgt('B' : code)) + ' ' +
  %char(%lookuplt('B' : code));
write QSYSPRT line;
*inlr = *on;
**CTDATA code
A00125B10000
C00001D99999
