**FREE
//; run: no
ctl-opt dftactgrp(*no);
begsr s1;
leavesr;
endsr;
begsr *inzsr;
endsr;
begsr *pssr;
endsr 'xyz';
*inlr = *on;
