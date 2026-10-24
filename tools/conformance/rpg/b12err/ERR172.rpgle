**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-s a int(10);
a = %diff(a : a : *days);
*inlr = *on;
