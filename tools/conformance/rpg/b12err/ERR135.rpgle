**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-ds d;
  a char(5);
end-ds;
dcl-s b like(d.a);
*inlr = *on;
