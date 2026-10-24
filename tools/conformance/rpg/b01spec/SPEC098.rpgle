**FREE
/title The free-form title
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/eject
line = 'free directives';
write QSYSPRT line;
*inlr = *on;
