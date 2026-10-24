**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-s c char(5);
c = %subst('abc':1:7);
*inlr = *on;
