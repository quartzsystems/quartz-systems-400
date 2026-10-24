**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n int(10) inz(42);
dsply 'covered message';
dsply ('n is ' + %char(n));
dsply n;
line = 'after dsply';
write QSYSPRT line;
*inlr = *on;
