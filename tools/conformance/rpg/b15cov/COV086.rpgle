**FREE
ctl-opt dftactgrp(*no) thread(*concurrent);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
callp one();
*inlr = *on;
dcl-proc one serialize;
  line = 'serialized';
  write QSYSPRT line;
end-proc;
