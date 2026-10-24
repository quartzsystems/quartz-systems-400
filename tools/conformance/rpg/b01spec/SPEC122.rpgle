**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s s varchar(200);
s = 'The quick brown fox jumps over the lazy dog and keeps on running +
     past the farm, over the hill and down to the river where it +
     finally stops';
line = %subst(s : 1 : 60);
write QSYSPRT line;
line = %subst(s : 61 : 60);
write QSYSPRT line;
line = %subst(s : 121) + ' ' + %char(%len(s));
write QSYSPRT line;
*inlr = *on;
