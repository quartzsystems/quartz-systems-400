**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-ds d;
  a int(10) pos(3) overlay(d);
end-ds;
*inlr = *on;
