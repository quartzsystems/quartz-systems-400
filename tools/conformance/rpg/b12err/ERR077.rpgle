**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-ds d dim(2);
  a int(10);
end-ds;
d.a = 1;
*inlr = *on;
