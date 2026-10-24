**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s f float(8) inz(1.5);
dcl-s g float(4) inz(-0.125);
dcl-s h float(8) inz(12345678.9);
line = '[' + %editflt(f) + ']';
write QSYSPRT line;
line = '[' + %editflt(g) + ']';
write QSYSPRT line;
line = '[' + %editflt(h) + ']';
write QSYSPRT line;
line = '[' + %editflt(0) + ']' + %char(%len(%editflt(f))) + ' ' + %char(%len(%editflt(g)));
write QSYSPRT line;
*inlr = *on;
