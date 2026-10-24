**FREE
//; dds: B8A
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f B8A workstn usropn maxdev(*file);
next(e) 'COVNODEV' B8A;
line = 'next ' + %char(%status(B8A)) + ' ' + %error;
write QSYSPRT line;
*inlr = *on;
