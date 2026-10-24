**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a packed(5:2);
dcl-s b packed(3:1) inz(12.34);
dcl-s r packed(9:4);
dcl-s d date;
eval(h) r = a * 2;
a = b + 1;
d = %date('2026-13-01' : *iso);
line = %char(r) + %char(a);
write QSYSPRT line;
*inlr = *on;
