**FREE
ctl-opt dftactgrp(*no) intprec(20);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a int(10) inz(2000000000);
dcl-s b int(20);
b = a * 3;
line = %char(b);
write QSYSPRT line;
*inlr = *on;
