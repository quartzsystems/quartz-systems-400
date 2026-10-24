**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s s varchar(60);
s = 'it''s +
     Bob''s +
     book';
line = s + ' ' + %char(%len(s));
write QSYSPRT line;
*inlr = *on;
