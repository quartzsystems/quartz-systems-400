**FREE
ctl-opt dftactgrp(*no) copyright('Quartz cover') dftname(COVX) enbpfrcol(*pep)
  optimize(*basic) prfdta(*nocol);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
line = 'object keywords';
write QSYSPRT line;
*inlr = *on;
