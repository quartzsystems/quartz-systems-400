**FREE
ctl-opt dftactgrp(*no) alwnull(*usrctl);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c char(5);
line = 'c ' + %nullind(c);
write QSYSPRT line;
*inlr = *on;
