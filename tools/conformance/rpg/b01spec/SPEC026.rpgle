**FREE
ctl-opt dftactgrp(*no) expropts(*resdecpos);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s r packed(7:2);
dcl-s a packed(5:0) inz(2);
dcl-s b packed(5:0) inz(3);
r = a / b * b;
line = %char(r);
write QSYSPRT line;
r = (a + 0.555) * b;
line = %char(r);
write QSYSPRT line;
*inlr = *on;
