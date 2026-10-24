**FREE
ctl-opt dftactgrp(*no) formsalign(*yes) ftrans(*none) altseq(*none);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
line = 'forms';
write QSYSPRT line;
*inlr = *on;
