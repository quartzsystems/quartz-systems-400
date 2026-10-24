**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds fl;
  f4 float(4) inz(0.1);
  f8 float(8) inz(0.1);
  f8b float(8) inz(-1E-5);
end-ds;
dcl-s pk packed(15:7);
pk = f4;
line = %char(f4) + ' ' + %char(f8) + ' ' + %char(f8b) + ' ' + %char(pk);
write QSYSPRT line;
pk = f8;
f4 = 1E10;
line = %char(pk) + ' ' + %char(f4) + ' ' + %char(%size(fl));
write QSYSPRT line;
*inlr = *on;
