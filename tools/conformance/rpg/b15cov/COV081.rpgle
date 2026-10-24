**FREE
ctl-opt dftactgrp(*no) genlvl(20) intprec(20);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a int(10) inz(100000);
dcl-s b int(10) inz(100000);
dcl-s c int(20);
c = a * b;
line = %char(c);
write QSYSPRT line;
*inlr = *on;
