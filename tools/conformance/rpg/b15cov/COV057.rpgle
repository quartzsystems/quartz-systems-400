**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s o object(*java:'java.lang.Object');
o = %this;
line = 'x';
write QSYSPRT line;
*inlr = *on;
