**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s t time inz(t'14.30.05');
dcl-s d date;
dcl-s z timestamp;
line = %char(t:*usa0);
write QSYSPRT line;
d = %date('26-3-5':*ymd);
z = %timestamp('2026-03-05-14.30.05');
line = %char(d) + %char(z);
write QSYSPRT line;
*inlr = *on;
