**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s u ucs2(5) inz('abc');
dcl-s w ucs2(5) inz('abd');
dcl-s c char(5) inz('abc');
line = 'eq ' + %char(u = c);
write QSYSPRT line;
line = 'lt ' + %char(u < w);
write QSYSPRT line;
line = 'lit ' + %char(u = 'abc');
write QSYSPRT line;
line = 'mix [' + %char(c + u) + ']';
write QSYSPRT line;
*inlr = *on;
