**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s total packed(9:2);
total = addup(1.5 : 2 : 'x');
line = %char(total);
write QSYSPRT line;
*inlr = *on;
dcl-proc addup;
  dcl-pi *n packed(9:2);
    a packed(7:2) value;
    b int(10) const;
    c char(1);
  end-pi;
  dcl-s work packed(9:2) static;
  dcl-s unusedl char(2);
  work = a + b;
  c = 'y';
  return work;
end-proc;
