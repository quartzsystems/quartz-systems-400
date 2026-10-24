**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-s a int(10) dim(3);
dcl-s i int(10);
i = %lookupge(1:a);
*inlr = *on;
