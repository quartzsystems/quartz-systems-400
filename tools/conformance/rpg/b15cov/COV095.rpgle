**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f QPRINT printer(132) extind(*inu1);
write QPRINT line;
line = 'u1 ' + *inu1 + ' open ' + %open(QPRINT);
write QSYSPRT line;
*inlr = *on;
