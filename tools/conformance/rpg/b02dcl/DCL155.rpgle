**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds inds len(99);
  exit ind pos(3);
  error ind pos(50);
  range char(3) pos(10);
end-ds;
dcl-s ptr pointer inz(%addr(*in));
dcl-ds ind2 based(ptr);
  in05 ind pos(5);
  in20 ind pos(20);
end-ds;
exit = *on;
range = '101';
*in05 = *on;
in20 = *on;
line = %subst(inds : 1 : 12) + ' ' + error + ' ' + in05 + ' ' + *in20 + ' ' +
  %char(%size(*in));
write QSYSPRT line;
*inlr = *on;
