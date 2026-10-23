**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-s a int(10);
a = 1 / 0;
*inlr = *on;
