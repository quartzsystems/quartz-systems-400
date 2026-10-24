**FREE
ctl-opt dftactgrp(*no) fltdiv(*yes);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s r packed(15:13);
dcl-s a packed(3:0) inz(2);
dcl-s b packed(3:0) inz(3);
r = a / b;
line = %editc(r:'X');
write QSYSPRT line;
*inlr = *on;
