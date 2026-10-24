**FREE
ctl-opt dftactgrp(*no) actgrp('COVAG') alloc(*teraspace) dclopt(*nochgdslen);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s p pointer;
dcl-s b char(10) based(p);
p = %alloc(10);
b = 'teraspace';
line = b;
write QSYSPRT line;
dealloc p;
*inlr = *on;
