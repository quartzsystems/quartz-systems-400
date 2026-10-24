**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-pr p;
  exsr int(10);
end-pr;
*inlr = *on;
