**FREE
//; run: no
//; dds: B8A
ctl-opt dftactgrp(*no);
dcl-f B8A workstn;
exfmt SCR1;
if *in03;
endif;
*inlr = *on;
