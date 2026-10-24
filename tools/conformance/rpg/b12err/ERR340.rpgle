**FREE
//; run: no
//; dds: B8H
ctl-opt dftactgrp(*no);
dcl-f B8H workstn;
exfmt SCR;
if *in03;
endif;
*inlr = *on;
