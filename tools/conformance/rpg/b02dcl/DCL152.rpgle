**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds raw;
  txt char(9) inz('012345678');
  z1 zoned(3:0) overlay(txt);
  z2 zoned(4:2) overlay(txt : 4);
  hx char(3) inz(x'12345F');
  pk packed(5:0) overlay(hx);
  ng char(2) inz(x'012D');
  pn packed(3:1) overlay(ng);
end-ds;
line = %char(z1) + ' ' + %char(z2) + ' ' + %char(pk) + ' ' + %char(pn);
write QSYSPRT line;
z2 = -1.5;
line = %subst(txt : 1 : 3) + ' ' + %char(z2);
write QSYSPRT line;
*inlr = *on;
