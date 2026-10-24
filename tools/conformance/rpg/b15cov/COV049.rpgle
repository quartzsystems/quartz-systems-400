**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s g graph(5);
dcl-s v vargraph(10);
dcl-s c char(10);
g = %graph('ab');
v = %graph('xyz');
c = %char(v);
line = 'graph ' + %char(%len(g)) + ' ' + %char(%size(g)) + ' ' + %char(%len(v)) + ' [' + c + ']';
write QSYSPRT line;
*inlr = *on;
