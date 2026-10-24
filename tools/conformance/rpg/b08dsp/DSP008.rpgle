**FREE
//; dds: B8H
//; keys: 4,10=ONE enter
//; keys: 4,10=TWO f6
//; keys: enter
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8H workstn indds(inds);
dcl-ds inds len(99);
  exit ind pos(3);
  six ind pos(6);
  thirty ind pos(30);
  showit ind pos(31);
end-ds;
dcl-s n int(10);
// INDARA with an indicator structure; *IN stays as it was.
*in30 = *on;
dou exit;
n += 1;
thirty = %rem(n: 2) = 0;
showit = n >= 3;
RES = 'n=' + %char(n) + ' six=' + six + ' in30=' + *in30 + ' in06=' + *in06 + ' inp=' + INP;
exfmt SCR;
enddo;
*inlr = *on;
