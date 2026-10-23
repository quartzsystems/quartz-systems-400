**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-pr q int(10);
  x int(10) value;
end-pr;
dcl-proc q;
  dcl-pi *n int(10);
    x char(5) value;
  end-pi;
  return 1;
end-proc;
*inlr = *on;
