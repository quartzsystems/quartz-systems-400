**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-proc p;
  dcl-pi *n;
    x int(10) value;
  end-pi;
end-proc;
p();
*inlr = *on;
