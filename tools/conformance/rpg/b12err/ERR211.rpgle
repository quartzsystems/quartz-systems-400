**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-proc p;
  begsr s;
  endsr;
  exsr t;
end-proc;
*inlr = *on;
