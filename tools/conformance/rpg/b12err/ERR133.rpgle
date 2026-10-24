**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-ds d psds;
  a *status;
end-ds;
dcl-ds e psds;
  b *status;
end-ds;
*inlr = *on;
