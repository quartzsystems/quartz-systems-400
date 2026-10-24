**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s r char(10) inz('before');
dcl-s n packed(3:0) inz(5);
dsply 'Reply please' '' r;
line = 'reply [' + r + ']';
write QSYSPRT line;
dsply 'Number' '*EXT' n;
line = 'number ' + %char(n);
write QSYSPRT line;
*inlr = *on;
