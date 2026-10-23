**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a packed(5:2) inz(1.5);
dcl-s b packed(5:2) inz(-2.25);
dcl-s c packed(5:2) inz(7);
line = %char(%max(a:b:c)) + ' ' + %char(%min(a:b:c));
write QSYSPRT line;
line = %char(%max(3:9:1)) + ' ' + %char(%min(3:9:1));
write QSYSPRT line;
line = %max('abc':'abd') + ' ' + %min('b':'a');
write QSYSPRT line;
*inlr = *on;
