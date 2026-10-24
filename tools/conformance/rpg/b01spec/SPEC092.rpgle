**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s s varchar(40);
s = 'abc+
     def';
line = '[' + s + ']';
write QSYSPRT line;
s = 'abc-
   def';
line = '[' + s + ']';
write QSYSPRT line;
*inlr = *on;
