**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-pr q extpgm('X');
end-pr;
dcl-s a int(10);
a = q();
*inlr = *on;
