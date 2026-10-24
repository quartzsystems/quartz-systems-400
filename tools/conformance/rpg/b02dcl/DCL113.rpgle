**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a packed(3:0) dim(3) inz(7);
dcl-s b char(2) dim(2) inz(*all'z');
dcl-ds s dim(2) qualified inz;
  x int(5) inz(4);
  y char(2);
end-ds;
a(2) = 1;
s(2).x = 9;
line = %char(a(1)) + %char(a(2)) + %char(a(3)) + ' ' + b(1) + b(2) + ' ' + %char(s(1).x) +
  %char(s(2).x) + '[' + s(1).y + ']';
write QSYSPRT line;
reset a;
reset s;
line = %char(a(1)) + %char(a(2)) + %char(a(3)) + ' ' + %char(s(1).x) + %char(s(2).x);
write QSYSPRT line;
*inlr = *on;
