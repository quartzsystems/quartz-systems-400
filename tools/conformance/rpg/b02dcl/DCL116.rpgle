**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-c N 3;
dcl-s a char(4) dim(N);
dcl-s b packed(7:2) dim(%elem(a));
dcl-s c int(10) dim(10);
dcl-s d like(b) dim(2);
dcl-s e varchar(5) dim(4);
dcl-s f date dim(2);
dcl-s g ind dim(99);
line = %char(%elem(a)) + ' ' + %char(%size(a)) + ' ' + %char(%size(a : *all));
write QSYSPRT line;
line = %char(%elem(b)) + ' ' + %char(%size(b : *all)) + ' ' + %char(%size(c : *all));
write QSYSPRT line;
line = %char(%elem(d)) + ' ' + %char(%size(d)) + ' ' + %char(%size(e)) + ' ' +
  %char(%size(e : *all));
write QSYSPRT line;
line = %char(%size(f : *all)) + ' ' + %char(%elem(g));
write QSYSPRT line;
*inlr = *on;
