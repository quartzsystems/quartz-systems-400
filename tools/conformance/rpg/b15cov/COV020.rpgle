**FREE
ctl-opt dftactgrp(*no) debug(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n int(10) inz(7);
dump;
dump 'one';
line = 'after dump ' + %char(n);
write QSYSPRT line;
*inlr = *on;
