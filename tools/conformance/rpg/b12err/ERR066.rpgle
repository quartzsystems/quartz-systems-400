**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-proc p;
  dcl-pi *n;
    x int(10);
  end-pi;
end-proc;
p(5);
*inlr = *on;
