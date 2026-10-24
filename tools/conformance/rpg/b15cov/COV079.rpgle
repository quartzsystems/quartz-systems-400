**FREE
ctl-opt dftactgrp(*no) fixnbr(*zoned:*inputpacked);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds ds;
  c char(3);
  z zoned(3:0) overlay(c);
end-ds;
dcl-s n packed(5:0);
c = ' 1 ';
n = z + 1;
line = 'fixed ' + %char(n);
write QSYSPRT line;
*inlr = *on;
