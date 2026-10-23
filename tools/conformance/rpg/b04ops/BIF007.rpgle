**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s amt packed(7:2) inz(1234.56);
dcl-s neg packed(7:2) inz(-12.3);
dcl-s date6 zoned(6:0) inz(102326);
line = '[' + %editw(amt:'   ,  0.  ') + ']';
write QSYSPRT line;
line = '[' + %editw(neg:'   ,  0.  -') + ']';
write QSYSPRT line;
line = '[' + %editw(neg:'   ,  0.  CR') + ']';
write QSYSPRT line;
line = '[' + %editw(amt:'$   ,  0.  ') + ']';
write QSYSPRT line;
line = '[' + %editw(date6:'  /  /  ') + ']';
write QSYSPRT line;
*inlr = *on;
