**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-proc p;
  dcl-pi *n;
    out int(10);
  end-pi;
end-proc;
*inlr = *on;
