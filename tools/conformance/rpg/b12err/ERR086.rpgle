**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-ds d len(40);
  p char(2) overlay(d : 30);
end-ds;
*inlr = *on;
