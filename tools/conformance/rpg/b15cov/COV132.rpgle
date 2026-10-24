**FREE
ctl-opt dftactgrp(*no) openopt(*inzofl) reqprexp(*warn);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
line = 'openopt reqprexp';
write QSYSPRT line;
*inlr = *on;
