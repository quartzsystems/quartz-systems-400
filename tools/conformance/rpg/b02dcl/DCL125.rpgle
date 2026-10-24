**FREE
//; dds: CUSTF
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds a extname('CUSTF' : *all) qualified end-ds;
dcl-ds i extname('CUSTF' : *input) qualified end-ds;
dcl-ds o extname('CUSTF' : *output) qualified end-ds;
dcl-ds k extname('CUSTF' : *key) qualified end-ds;
k.cusno = 101;
a.name = 'all';
line = %char(%size(a)) + ' ' + %char(%size(i)) + ' ' + %char(%size(o)) + ' ' +
  %char(%size(k));
write QSYSPRT line;
line = %char(k.cusno) + ' ' + %trim(a.name) + ' ' + %char(%size(k.cusno));
write QSYSPRT line;
*inlr = *on;
