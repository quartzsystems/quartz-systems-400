**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a int(10);
a = x + y + z;
dcl-s q char(2) inz('abc'); dcl-s w int(5) inz(99999999);
*inlr = *on;
