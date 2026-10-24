**FREE
ctl-opt dftactgrp(*no) ccsid(*graph:835);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c char(4) inz('ab');
dcl-s g graph(4);
g = %graph(c);
line = 'graph ' + %char(%len(g)) + ' ' + %char(%size(g));
write QSYSPRT line;
*inlr = *on;
