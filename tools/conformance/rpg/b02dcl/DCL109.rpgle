**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-c K 25;
dcl-c TXT 'const';
dcl-s arr char(6) dim(4);
dcl-s a int(10) inz(%size(arr));
dcl-s b int(10) inz(%elem(arr));
dcl-s c int(10) inz(%len(TXT));
dcl-s d packed(5:1) inz(K);
dcl-s e char(8) inz(TXT);
dcl-s f int(10) inz(%size(arr : *all));
dcl-s g int(10) inz(%decpos(d));
line = %char(a) + ' ' + %char(b) + ' ' + %char(c) + ' ' + %char(d) + ' ' + e + ' ' +
  %char(f) + ' ' + %char(g);
write QSYSPRT line;
*inlr = *on;
