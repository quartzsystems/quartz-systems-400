**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-ds d;
  a int(10) overlay(d : *next);
end-ds;
*inlr = *on;
