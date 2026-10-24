**FREE
//; run: no
//; dds: B8H
ctl-opt dftactgrp(*no);
dcl-f B8H workstn indds(inds);
dcl-ds inds len(99);
  ex ind pos(3);
end-ds;
exfmt SCR;
if *in03;
endif;
*inlr = *on;
