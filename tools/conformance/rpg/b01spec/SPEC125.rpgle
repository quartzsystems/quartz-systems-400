**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-c K 6;
dcl-s base char(K) inz('base');
dcl-s arr int(5) dim(K);
dcl-s cpy char(%size(base));
dcl-s arr2 packed(3:0) dim(%elem(arr));
dcl-s sz int(10) inz(%size(arr));
dcl-s ln int(10) inz(%len(base));
line = %char(%size(cpy)) + ' ' + %char(%elem(arr2)) + ' ' + %char(sz) + ' ' + %char(ln);
write QSYSPRT line;
*inlr = *on;
