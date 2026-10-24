**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-pr q int(10) extpgm('X') extproc('Y');
end-pr;
*inlr = *on;
