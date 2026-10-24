**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
read QSYSPRT;
chain 1 QSYSPRT;
*inlr = *on;
