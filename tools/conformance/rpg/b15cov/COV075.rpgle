**FREE
ctl-opt dftactgrp(*no) datedit(*ymd-);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s d zoned(6:0) inz(261024);
dcl-s e zoned(8:0) inz(20261024);
line = %editc(d:'Y') + ' ' + %editc(e:'Y');
write QSYSPRT line;
*inlr = *on;
