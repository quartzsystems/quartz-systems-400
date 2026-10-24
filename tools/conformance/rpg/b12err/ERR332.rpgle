**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-pr p extpgm('X');
  i ind const;
end-pr;
p(*in50);
if *in50;
endif;
*inlr = *on;
