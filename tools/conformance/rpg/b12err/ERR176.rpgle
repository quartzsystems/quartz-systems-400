**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-s p pointer(*proc);
p = %paddr(nosuch);
*inlr = *on;
