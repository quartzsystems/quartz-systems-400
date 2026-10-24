**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-c K %len('abc');
dcl-s a char(K);
dcl-s b char(%len(a));
*inlr = *on;
