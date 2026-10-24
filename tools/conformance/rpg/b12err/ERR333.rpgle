**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-s p pointer;
p = %addr(*in);
if *in50;
endif;
*inlr = *on;
