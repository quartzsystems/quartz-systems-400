**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
callp work();
line = 'back in main';
write QSYSPRT line;
*inlr = *on;
dcl-proc work;
  line = 'working';
  write QSYSPRT line;
  return;
  on-exit;
    line = 'on-exit runs';
    write QSYSPRT line;
end-proc;
