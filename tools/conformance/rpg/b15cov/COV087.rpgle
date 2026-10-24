**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
callp one();
*inlr = *on;
dcl-proc one serialize;
  line = 'x';
  write QSYSPRT line;
end-proc;
