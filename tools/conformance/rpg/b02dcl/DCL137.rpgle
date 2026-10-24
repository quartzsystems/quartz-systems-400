**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds al align qualified;
  c1 char(1);
  i2 int(5);
  c3 char(1);
  i4 int(10);
  c5 char(3);
  i8 int(20);
  f4 float(4);
  c6 char(1);
  f8 float(8);
  u2 uns(5);
  b4 bindec(9);
  pt pointer;
end-ds;
dcl-ds na qualified;
  c1 char(1);
  i4 int(10);
  pt pointer;
end-ds;
line = %char(%size(al)) + ' ' + %char(%addr(al.i2) - %addr(al)) + ' ' + %char(%addr(al.i4)
  - %addr(al)) + ' ' + %char(%addr(al.i8) - %addr(al));
write QSYSPRT line;
line = %char(%addr(al.f4) - %addr(al)) + ' ' + %char(%addr(al.f8) - %addr(al)) + ' ' +
  %char(%addr(al.u2) - %addr(al)) + ' ' + %char(%addr(al.b4) - %addr(al));
write QSYSPRT line;
line = %char(%addr(al.pt) - %addr(al)) + ' ' + %char(%size(na)) + ' ' + %char(%addr(na.pt)
  - %addr(na));
write QSYSPRT line;
*inlr = *on;
