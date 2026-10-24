**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds raw;
  c4 char(4) inz(x'00000102');
  i4 int(10) overlay(c4);
  u2 uns(5) overlay(c4 : 3);
  b2 bindec(4:0) overlay(c4 : 3);
  c8 char(8) inz(x'FFFFFFFFFFFFFFFE');
  i8 int(20) overlay(c8);
  u8 uns(20) overlay(c8);
  i1 int(3) overlay(c8 : 8);
end-ds;
line = %char(i4) + ' ' + %char(u2) + ' ' + %char(b2) + ' ' + %char(i8) + ' ' + %char(u8) +
  ' ' + %char(i1);
write QSYSPRT line;
*inlr = *on;
