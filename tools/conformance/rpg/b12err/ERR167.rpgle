**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-s c char(5);
c = %xlate('a' : 'bc' : c);
*inlr = *on;
