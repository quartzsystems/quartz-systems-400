**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-proc p;
  begsr *pssr;
  endsr;
  begsr *inzsr;
  endsr;
end-proc;
*inlr = *on;
